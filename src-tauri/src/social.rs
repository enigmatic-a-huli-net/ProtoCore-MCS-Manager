//! Игроки (история из консоли) и уведомления в Telegram/Discord, плюс Telegram-бот.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pcm_core::notify::{self, BotCmd};
use pcm_core::players::{Event, PlayerDb};
use pcm_core::process::{ExitInfo, LogLine, Status, StatusInfo};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

#[derive(Default)]
pub struct Social {
    pub dbs: Mutex<HashMap<String, PlayerDb>>,
    dirty: Mutex<HashSet<String>>,
    last_status: Mutex<HashMap<String, Status>>,
    /// когда последний раз слали уведомление по ключу (защита от спама)
    sent: Mutex<HashMap<String, Instant>>,
    /// сколько похожих уведомлений подавлено с последней отправки
    suppressed: Mutex<HashMap<String, u32>>,
    /// отложено на конец тихих часов
    held: Mutex<Vec<String>>,
    /// бот: ожидающие подтверждения команды (nonce → команда, кто, когда)
    pending: Mutex<HashMap<String, (BotCmd, String, Instant)>>,
    /// бот: время последних команд каждого пользователя
    rate: Mutex<HashMap<String, Vec<Instant>>>,
    /// когда последний раз играли звук по ключу
    sounded: Mutex<HashMap<String, Instant>>,
}

/// Звуковое оповещение (не чаще раза в `every` по одному ключу).
pub fn sound(app: &AppHandle, s: pcm_core::sound::Sound, key: &str, every: Duration) {
    let st = app.state::<AppState>();
    let settings = st.config.lock().unwrap().sounds.clone();
    if !s.enabled_in(&settings) {
        return;
    }
    {
        let social = app.state::<Social>();
        let mut m = social.sounded.lock().unwrap();
        if m.get(key).is_some_and(|t| t.elapsed() < every) {
            return;
        }
        m.insert(key.to_string(), Instant::now());
    }
    pcm_core::sound::play(s, &settings, &st.paths.branding, false);
}

fn minute_now() -> u32 {
    use chrono::Timelike;
    let n = chrono::Local::now();
    n.hour() * 60 + n.minute()
}

fn is_test(id: &str) -> bool {
    id.ends_with("~test")
}

fn server_name(app: &AppHandle, id: &str) -> String {
    app.state::<AppState>().config.lock().unwrap().server(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string())
}

/// Уведомление в фоне. `key` + `every` — не чаще одного раза за интервал.
pub fn notify(app: &AppHandle, key: &str, every: Duration, text: String) {
    // предупреждения (бэкап, диск, TPS, сеть) — ещё и звуком
    let kind = key.split(':').next().unwrap_or("");
    if !matches!(kind, "crash" | "start" | "stop" | "join") {
        sound(app, pcm_core::sound::Sound::Warn, key, every.min(Duration::from_secs(600)));
    }
    let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
    if !s.any() {
        return;
    }
    let every = every.max(Duration::from_secs(s.min_interval_min as u64 * 60));
    let social = app.state::<Social>();
    // тихие часы: копим и пришлём сводкой утром (падения — по настройке сразу)
    let urgent = key.starts_with("crash:") && s.quiet_allow_crash;
    if !urgent && s.quiet_at(minute_now()) {
        let mut held = social.held.lock().unwrap();
        if held.len() < 50 {
            held.push(text);
        }
        return;
    }
    let mut text = text;
    {
        let mut sent = social.sent.lock().unwrap();
        let mut sup = social.suppressed.lock().unwrap();
        if sent.get(key).is_some_and(|t| t.elapsed() < every) {
            // не спамим: считаем, сколько похожих пропустили
            *sup.entry(key.to_string()).or_default() += 1;
            return;
        }
        sent.insert(key.to_string(), Instant::now());
        if let Some(n) = sup.remove(key).filter(|n| *n > 0) {
            text.push_str(&format!("\n(и ещё {n} таких же с прошлого уведомления)"));
        }
    }
    std::thread::spawn(move || {
        let _ = notify::send(&s, &text);
    });
}

/// После тихих часов — одна сводка вместо пачки сообщений (вызывается планировщиком).
pub fn release_held(app: &AppHandle) {
    let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
    if s.quiet_at(minute_now()) {
        return;
    }
    let held: Vec<String> = std::mem::take(&mut *app.state::<Social>().held.lock().unwrap());
    if held.is_empty() || !s.any() {
        return;
    }
    let mut text = format!("🌙 За тихие часы ({}):\n", held.len());
    for h in held.iter().take(15) {
        text.push_str(&format!("• {}\n", h.lines().next().unwrap_or("")));
    }
    if held.len() > 15 {
        text.push_str(&format!("…и ещё {}", held.len() - 15));
    }
    std::thread::spawn(move || {
        let _ = notify::send(&s, &text);
    });
}

pub fn with_db<T>(app: &AppHandle, id: &str, f: impl FnOnce(&mut PlayerDb) -> T) -> T {
    let state = app.state::<AppState>();
    let social = app.state::<Social>();
    let mut dbs = social.dbs.lock().unwrap();
    let db = dbs.entry(id.to_string()).or_insert_with(|| PlayerDb::load(&PlayerDb::file(&state.paths.data, id)));
    let salt = {
        let mut c = state.config.lock().unwrap();
        if c.hash_player_ips && c.ip_salt.is_empty() {
            c.ip_salt = pcm_core::util::random_password();
        }
        c.hash_player_ips.then(|| c.ip_salt.clone())
    };
    let was = db.ip_salt.is_some();
    db.ip_salt = salt;
    if !was && db.hash_existing() {
        app.state::<Social>().dirty.lock().unwrap().insert(id.to_string());
    }
    f(db)
}

fn mark(app: &AppHandle, id: &str) {
    app.state::<Social>().dirty.lock().unwrap().insert(id.to_string());
}

/// Сохранить изменённые базы игроков (вызывается планировщиком).
pub fn flush(app: &AppHandle) {
    let ids: Vec<String> = app.state::<Social>().dirty.lock().unwrap().drain().collect();
    let data = app.state::<AppState>().paths.data.clone();
    for id in ids {
        let db = app.state::<Social>().dbs.lock().unwrap().get(&id).cloned();
        if let Some(db) = db {
            let _ = db.save(&PlayerDb::file(&data, &id));
        }
    }
}

pub fn audit(app: &AppHandle, id: &str, cmd: &str, by: &str) {
    with_db(app, id, |db| db.audit(cmd, by));
    mark(app, id);
}

// ---------------------------------------------------------------- события процесса

pub fn on_log(app: &AppHandle, id: &str, lines: &[LogLine]) {
    if is_test(id) {
        return;
    }
    let mut joined = Vec::new();
    let mut changed = false;
    for l in lines {
        if let Some(ev) = pcm_core::players::parse(&l.text) {
            if let Event::Join { name } = &ev {
                joined.push(name.clone());
            }
            changed |= with_db(app, id, |db| db.apply(&ev));
        }
    }
    if changed {
        mark(app, id);
    }
    if !joined.is_empty() {
        sound(app, pcm_core::sound::Sound::Join, &format!("join:{id}"), Duration::from_secs(3));
    }
    if !joined.is_empty() && app.state::<AppState>().config.lock().unwrap().notify.on_join {
        let n = server_name(app, id);
        for p in joined {
            notify(app, &format!("join:{id}:{p}"), Duration::from_secs(60), format!("👋 {p} зашёл на «{n}»"));
        }
    }
}

pub fn on_status(app: &AppHandle, id: &str, info: &StatusInfo) {
    if is_test(id) {
        return;
    }
    let prev = app.state::<Social>().last_status.lock().unwrap().insert(id.to_string(), info.status);
    if info.status == Status::Running && prev != Some(Status::Running) {
        sound(app, pcm_core::sound::Sound::Start, &format!("start:{id}"), Duration::from_secs(5));
        let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
        if s.on_start {
            notify(app, &format!("start:{id}"), Duration::from_secs(30), format!("✅ «{}» запущен", server_name(app, id)));
        }
    }
}

pub fn on_exit(app: &AppHandle, id: &str, info: &ExitInfo) {
    if is_test(id) {
        return;
    }
    with_db(app, id, |db| db.close_all());
    mark(app, id);
    let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
    let n = server_name(app, id);
    if info.crashed {
        sound(app, pcm_core::sound::Sound::Crash, &format!("crash:{id}"), Duration::from_secs(20));
    } else {
        sound(app, pcm_core::sound::Sound::Stop, &format!("stop:{id}"), Duration::from_secs(5));
    }
    if info.crashed && s.on_crash {
        let code = info.code.map(|c| format!(" (код {c})")).unwrap_or_default();
        let when = if info.reached_running { "во время работы" } else { "при запуске" };
        notify(app, &format!("crash:{id}"), Duration::from_secs(60), format!("💥 «{n}» упал {when}{code}. Подробности — во вкладке «Диагностика»."));
    } else if !info.crashed && s.on_stop {
        notify(app, &format!("stop:{id}"), Duration::from_secs(30), format!("■ «{n}» остановлен"));
    }
}

// ---------------------------------------------------------------- Telegram-бот

fn find_server(app: &AppHandle, q: Option<&str>) -> Result<pcm_core::config::ServerEntry, String> {
    let c = app.state::<AppState>().config.lock().unwrap().clone();
    // сервер указывается всегда — номером из /status или названием
    let list = || {
        c.servers.iter().enumerate().map(|(i, s)| format!("{}. {}", i + 1, s.name)).collect::<Vec<_>>().join("\n")
    };
    let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) else {
        return Err(format!("Укажи сервер номером или названием, например /restart 1\n{}", list()));
    };
    if let Ok(n) = q.parse::<usize>() {
        return c.servers.get(n.wrapping_sub(1)).cloned().ok_or_else(|| format!("Нет сервера №{n}\n{}", list()));
    }
    let ql = q.to_lowercase();
    if let Some(s) = c.servers.iter().find(|s| s.name.to_lowercase() == ql) {
        return Ok(s.clone());
    }
    let hits: Vec<_> = c.servers.iter().filter(|s| s.name.to_lowercase().contains(&ql)).collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => Err(format!("Сервер «{q}» не найден\n{}", list())),
        _ => Err(format!("Под «{q}» подходят несколько — укажи номер:\n{}", list())),
    }
}

fn status_text(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let c = state.config.lock().unwrap().clone();
    let tps = app.state::<crate::scheduler::Sched>();
    let mut out = String::new();
    for s in &c.servers {
        let st = state.pm.status(&s.id);
        let icon = match st.status {
            Status::Running => "🟢",
            Status::Starting | Status::Stopping => "🟡",
            Status::Crashed => "🔴",
            Status::Stopped => "⚪",
        };
        let t = tps.servers.lock().unwrap().get(&s.id).and_then(|x| x.history.back().cloned());
        let extra = if st.status == Status::Running {
            let up = st.uptime;
            format!(
                " · игроков {} · {}аптайм {}ч {}м",
                st.players.len(),
                t.map(|t| format!("TPS {:.1} · ", t.tps)).unwrap_or_default(),
                up / 3600,
                (up % 3600) / 60
            )
        } else {
            String::new()
        };
        out.push_str(&format!("{}. {icon} {}{extra}\n", out.lines().count() + 1, s.name));
    }
    if out.is_empty() {
        "Серверов нет".into()
    } else {
        out
    }
}

fn handle(app: &AppHandle, cmd: BotCmd) -> String {
    let state = app.state::<AppState>();
    match cmd {
        BotCmd::Help => notify::BOT_HELP.into(),
        BotCmd::Id => String::new(), // обрабатывается в цикле бота
        BotCmd::Status => status_text(app),
        BotCmd::Unknown(c) => format!("Не знаю команду /{c}. /help — список команд"),
        BotCmd::Players(q) => match find_server(app, q.as_deref()) {
            Ok(s) => {
                let p = state.pm.status(&s.id).players;
                if p.is_empty() {
                    format!("На «{}» никого", s.name)
                } else {
                    format!("«{}», онлайн {}: {}", s.name, p.len(), p.join(", "))
                }
            }
            Err(e) => e,
        },
        BotCmd::Say(_, text) => {
            // «/say 1 привет» или «/say lobby привет»
            let mut it = text.trim().splitn(2, char::is_whitespace);
            let q = it.next().map(String::from);
            let text = it.next().unwrap_or("").to_string();
            match find_server(app, q.as_deref()) {
            Ok(s) if !text.trim().is_empty() => {
                let clean: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
                match state.pm.send(&s.id, &format!("say [Telegram] {clean}")) {
                    Ok(()) => {
                        audit(app, &s.id, &format!("say {clean}"), "bot");
                        "Отправлено".into()
                    }
                    Err(e) => e.to_string(),
                }
            }
            Ok(_) => "Напиши текст: /say 1 привет".into(),
            Err(e) => e,
            }
        }
        BotCmd::Start(ref q) | BotCmd::Stop(ref q) | BotCmd::Restart(ref q) | BotCmd::Backup(ref q) => {
            let s = match find_server(app, q.as_deref()) {
                Ok(s) => s,
                Err(e) => return e,
            };
            let app2 = app.clone();
            let id = s.id.clone();
            let name = s.name.clone();
            let (verb, job): (&str, Box<dyn FnOnce() -> Result<(), String> + Send>) = match cmd {
                BotCmd::Start(_) => ("Запускаю", Box::new(move || crate::commands::start_blocking(&app2, &id, true))),
                BotCmd::Restart(_) => ("Перезапускаю", Box::new(move || crate::commands::restart_blocking(&app2, &id))),
                BotCmd::Backup(_) => ("Делаю снапшот", Box::new(move || crate::commands::backup_blocking(&app2, &id, "bot").map(|_| ()))),
                _ => (
                    "Останавливаю",
                    Box::new(move || {
                        let st = app2.state::<AppState>();
                        st.pm.stop_and_wait(&id, Duration::from_secs(90)).map_err(|e| e.to_string())
                    }),
                ),
            };
            audit(app, &s.id, &format!("{verb} (бот)"), "bot");
            let app3 = app.clone();
            std::thread::spawn(move || {
                let r = job();
                let s = app3.state::<AppState>().config.lock().unwrap().notify.clone();
                let text = match r {
                    Ok(()) => format!("Готово: «{name}»"),
                    Err(e) => format!("«{name}»: {e}"),
                };
                let _ = notify::telegram_send(&s.telegram_token, &s.telegram_chat, &text);
            });
            format!("{verb} «{}»…", s.name)
        }
    }
}

fn cmd_label(cmd: &BotCmd) -> &'static str {
    match cmd {
        BotCmd::Start(_) => "запустить",
        BotCmd::Stop(_) => "остановить",
        BotCmd::Restart(_) => "перезапустить",
        _ => "выполнить",
    }
}

fn cmd_server(cmd: &BotCmd) -> Option<&str> {
    match cmd {
        BotCmd::Start(q) | BotCmd::Stop(q) | BotCmd::Restart(q) | BotCmd::Backup(q) | BotCmd::Players(q) => q.as_deref(),
        _ => None,
    }
}

/// Лимит: не больше 6 команд в минуту от одного человека.
fn rate_ok(app: &AppHandle, user: &str) -> bool {
    let social = app.state::<Social>();
    let mut r = social.rate.lock().unwrap();
    let v = r.entry(user.to_string()).or_default();
    v.retain(|t| t.elapsed() < Duration::from_secs(60));
    if v.len() >= 6 {
        return false;
    }
    v.push(Instant::now());
    true
}

fn process(app: &AppHandle, s: &notify::NotifySettings, u: notify::Update) {
    let token = &s.telegram_token;
    let private = u.chat == u.from;
    // право управлять: в списке администраторов; если список пуст — только владелец в личном чате
    let allowed = s.is_admin(&u.from) || (s.telegram_admins.is_empty() && private);

    // нажатие кнопки подтверждения
    if let Some((cb_id, data, msg_id)) = u.callback {
        let (ok, nonce) = match data.split_once(':') {
            Some(("ok", n)) => (true, n.to_string()),
            Some(("no", n)) => (false, n.to_string()),
            _ => return,
        };
        let pend = app.state::<Social>().pending.lock().unwrap().remove(&nonce);
        let Some((cmd, who, at)) = pend else {
            notify::telegram_answer(token, &cb_id, "Устарело");
            notify::telegram_edit(token, &u.chat, msg_id, "⌛ Подтверждение устарело — отправь команду заново");
            return;
        };
        if who != u.from {
            // чужое подтверждение — возвращаем ожидание обратно
            app.state::<Social>().pending.lock().unwrap().insert(nonce, (cmd, who, at));
            notify::telegram_answer(token, &cb_id, "Подтвердить может только тот, кто отправил команду");
            return;
        }
        if !ok {
            notify::telegram_answer(token, &cb_id, "Отменено");
            notify::telegram_edit(token, &u.chat, msg_id, "✖ Отменено");
            return;
        }
        if at.elapsed() > Duration::from_secs(120) || !allowed {
            notify::telegram_answer(token, &cb_id, "Нельзя");
            notify::telegram_edit(token, &u.chat, msg_id, "⌛ Подтверждение устарело — отправь команду заново");
            return;
        }
        notify::telegram_answer(token, &cb_id, "Ок");
        let reply = handle(app, cmd);
        notify::telegram_edit(token, &u.chat, msg_id, &format!("{reply} (подтвердил {})", u.from_name));
        return;
    }

    let Some(cmd) = notify::parse_cmd(&u.text) else { return };
    if !rate_ok(app, &u.from) {
        return;
    }
    if cmd == BotCmd::Id {
        let _ = notify::telegram_send(token, &u.chat, &format!("Твой Telegram id: {}\nId этого чата: {}", u.from, u.chat));
        return;
    }
    if cmd.dangerous() && !allowed {
        let _ = notify::telegram_send(
            token,
            &u.chat,
            "⛔ Управлять серверами могут только администраторы. Напиши /id и попроси владельца добавить твой id в настройках менеджера.",
        );
        return;
    }
    if cmd.needs_confirm() {
        // сначала проверим, что сервер указан верно, чтобы не спрашивать зря
        if let Err(e) = find_server(app, cmd_server(&cmd)) {
            let _ = notify::telegram_send(token, &u.chat, &e);
            return;
        }
        let name = find_server(app, cmd_server(&cmd)).map(|x| x.name).unwrap_or_default();
        let nonce = pcm_core::util::short_id();
        {
            let social = app.state::<Social>();
            let mut p = social.pending.lock().unwrap();
            p.retain(|_, v| v.2.elapsed() < Duration::from_secs(300));
            p.insert(nonce.clone(), (cmd.clone(), u.from.clone(), Instant::now()));
        }
        let _ = notify::telegram_send_buttons(
            token,
            &u.chat,
            &format!("{}: {} «{name}»?", u.from_name, cmd_label(&cmd)),
            &[("✅ Да", format!("ok:{nonce}")), ("✖ Отмена", format!("no:{nonce}"))],
        );
        return;
    }
    let reply = handle(app, cmd);
    if !reply.is_empty() {
        let _ = notify::telegram_send(token, &u.chat, &reply);
    }
}

pub fn start_bot(app: AppHandle) {
    std::thread::Builder::new()
        .name("pcm-telegram".into())
        .spawn(move || {
            let mut offset: i64 = 0;
            let mut token_seen = String::new();
            loop {
                let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
                if !s.telegram_bot || !s.telegram_ready() {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
                if token_seen != s.telegram_token {
                    // новый токен — пропускаем старые сообщения, чтобы не выполнить их задним числом
                    token_seen = s.telegram_token.clone();
                    offset = notify::telegram_updates(&s.telegram_token, -1, 0).map(|(o, _)| o).unwrap_or(0);
                }
                match notify::telegram_updates(&s.telegram_token, offset, 25) {
                    Ok((next, ups)) => {
                        offset = next;
                        let chat = notify::normalize_chat(&s.telegram_chat);
                        for u in ups {
                            // только «свой» чат — остальным бот не отвечает вообще
                            if u.chat != chat {
                                continue;
                            }
                            process(&app, &s, u);
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_secs(15)),
                }
            }
        })
        .ok();
}
