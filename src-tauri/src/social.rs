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
}

fn is_test(id: &str) -> bool {
    id.ends_with("~test")
}

fn server_name(app: &AppHandle, id: &str) -> String {
    app.state::<AppState>().config.lock().unwrap().server(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string())
}

/// Уведомление в фоне. `key` + `every` — не чаще одного раза за интервал.
pub fn notify(app: &AppHandle, key: &str, every: Duration, text: String) {
    let s = app.state::<AppState>().config.lock().unwrap().notify.clone();
    if !s.any() {
        return;
    }
    {
        let social = app.state::<Social>();
        let mut sent = social.sent.lock().unwrap();
        if sent.get(key).is_some_and(|t| t.elapsed() < every) {
            return;
        }
        sent.insert(key.to_string(), Instant::now());
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
                    Ok((next, msgs)) => {
                        offset = next;
                        for (chat, text) in msgs {
                            // только «свой» чат — остальным бот не отвечает вообще
                            if chat != s.telegram_chat.trim() {
                                continue;
                            }
                            if let Some(cmd) = notify::parse_cmd(&text) {
                                let reply = handle(&app, cmd);
                                let _ = notify::telegram_send(&s.telegram_token, &chat, &reply);
                            }
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_secs(15)),
                }
            }
        })
        .ok();
}
