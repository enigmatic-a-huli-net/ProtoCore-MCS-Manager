//! Фоновый планировщик: раз в 5 секунд смотрит на все серверы.
//! - опрос TPS/MSPT (тихо, без мусора в консоли);
//! - плановые рестарты с предупреждениями игрокам;
//! - ночные бэкапы.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use pcm_core::process::Status;
use pcm_core::schedule;
use pcm_core::tps::{self, Tps};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const HISTORY: usize = 240;

#[derive(Default)]
pub struct SrvSched {
    /// для какого момента рестарта уже отправлены предупреждения
    due: Option<DateTime<Local>>,
    warned: Vec<u32>,
    postponed_until: Option<DateTime<Local>>,
    last_backup_day: String,
    last_tps: Option<Instant>,
    bad_cmds: HashSet<String>,
    pub history: VecDeque<Tps>,
    /// pid процесса, для которого собирались данные (сбрасываем при новом запуске)
    pid: Option<u32>,
}

#[derive(Default)]
pub struct Sched {
    pub servers: Mutex<HashMap<String, SrvSched>>,
}

#[derive(Clone, Serialize)]
struct TpsEvent<'a> {
    id: &'a str,
    tps: &'a Tps,
}

#[derive(Clone, Serialize)]
pub struct MonitorInfo {
    pub history: Vec<Tps>,
    pub next_restart: Option<i64>,
    pub next_backup: Option<i64>,
}

pub fn info(app: &AppHandle, id: &str) -> MonitorInfo {
    let state = app.state::<AppState>();
    let sched = app.state::<Sched>();
    let srv = state.config.lock().unwrap().server(id).cloned();
    let st = state.pm.status(id);
    let now = Local::now();
    let mut out = MonitorInfo { history: vec![], next_restart: None, next_backup: None };
    if let Some(s) = srv {
        if st.status == Status::Running || st.status == Status::Starting {
            let started = now - chrono::Duration::seconds(st.uptime as i64);
            let m = sched.servers.lock().unwrap();
            let post = m.get(id).and_then(|x| x.postponed_until);
            out.next_restart = post.or_else(|| schedule::next_restart(&s.schedule, started, now)).map(|d| d.timestamp_millis());
        }
        if s.schedule.backup_enabled {
            if let Some(t) = schedule::parse_hhmm(&s.schedule.backup_time) {
                let mut d = now.date_naive().and_time(t);
                if d <= now.naive_local() {
                    d += chrono::Duration::days(1);
                }
                out.next_backup = d.and_local_timezone(Local).single().map(|x| x.timestamp_millis());
            }
        }
    }
    if let Some(x) = sched.servers.lock().unwrap().get(id) {
        out.history = x.history.iter().cloned().collect();
    }
    out
}

fn has_spark(dir: &std::path::Path) -> bool {
    ["mods", "plugins"].iter().any(|d| {
        std::fs::read_dir(dir.join(d))
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().to_lowercase().starts_with("spark"))
    })
}

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("pcm-scheduler".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            tick(&app);
        })
        .ok();
}

fn disk_free_gb(path: &std::path::Path) -> Option<f64> {
    let disks = pcm_core::sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space() as f64 / 1024f64.powi(3))
}

fn alerts(app: &AppHandle) {
    let state = app.state::<AppState>();
    let n = state.config.lock().unwrap().notify.clone();
    if !n.any() {
        return;
    }
    let servers = state.config.lock().unwrap().servers.clone();
    // свободное место — раз в 5 минут
    static LAST_DISK: Mutex<Option<Instant>> = Mutex::new(None);
    let disk_due = LAST_DISK.lock().unwrap().map_or(true, |t| t.elapsed() > Duration::from_secs(300));
    if n.on_disk_low && disk_due {
        *LAST_DISK.lock().unwrap() = Some(Instant::now());
        for s in &servers {
            if let Some(free) = disk_free_gb(&s.dir()) {
                if free < n.disk_low_gb as f64 {
                    crate::social::notify(app, &format!("disk:{}", s.id), Duration::from_secs(6 * 3600), format!("💽 Мало места у «{}»: свободно {:.1} ГБ", s.name, free));
                }
            }
        }
    }
    // низкий TPS: два замера подряд ниже порога
    if n.on_low_tps {
        let sched = app.state::<Sched>();
        for s in &servers {
            let low = sched
                .servers
                .lock()
                .unwrap()
                .get(&s.id)
                .map(|x| {
                    let h: Vec<_> = x.history.iter().rev().take(2).collect();
                    h.len() == 2 && h.iter().all(|t| t.tps < n.low_tps)
                })
                .unwrap_or(false);
            if low {
                let t = sched.servers.lock().unwrap()[&s.id].history.back().map(|x| x.tps).unwrap_or(0.0);
                crate::social::notify(app, &format!("tps:{}", s.id), Duration::from_secs(900), format!("🐢 «{}»: TPS упал до {:.1}", s.name, t));
            }
        }
    }
    // сеть: потери за минуту
    if n.on_net_bad {
        let r = crate::netmon::report(app);
        if r.last_min.samples >= 20 && r.last_min.loss >= 5.0 {
            crate::social::notify(app, "net", Duration::from_secs(1800), format!("📶 Потери пакетов {:.0}% за последнюю минуту — у игроков будут лаги", r.last_min.loss));
        }
    }
}

fn tick(app: &AppHandle) {
    crate::social::flush(app);
    alerts(app);
    let state = app.state::<AppState>();
    let sched = app.state::<Sched>();
    let servers = state.config.lock().unwrap().servers.clone();
    let now = Local::now();

    for s in servers {
        let st = state.pm.status(&s.id);
        let running = st.status == Status::Running;

        // новый процесс — сбрасываем данные прошлого запуска
        {
            let mut m = sched.servers.lock().unwrap();
            let e = m.entry(s.id.clone()).or_default();
            if e.pid != st.pid {
                e.pid = st.pid;
                e.due = None;
                e.warned.clear();
                e.postponed_until = None;
                e.bad_cmds.clear();
                if st.pid.is_some() {
                    e.history.clear();
                }
            }
        }

        // ---------- TPS
        if running && s.schedule.tps_enabled {
            let due = {
                let m = sched.servers.lock().unwrap();
                let e = &m[&s.id];
                e.last_tps.map_or(true, |t| t.elapsed() >= Duration::from_secs(s.schedule.tps_interval_sec.max(10) as u64))
            };
            if due {
                let cmds: Vec<&str> = {
                    let m = sched.servers.lock().unwrap();
                    let bad = &m[&s.id].bad_cmds;
                    tps::commands(s.core, has_spark(&s.dir())).into_iter().filter(|c| !bad.contains(*c)).collect()
                };
                sched.servers.lock().unwrap().get_mut(&s.id).unwrap().last_tps = Some(Instant::now());
                if let Some(cmd) = cmds.first() {
                    // spark отвечает из своего потока — даём ему больше времени
                    let window = Duration::from_millis(if cmd.starts_with("spark") { 4000 } else { 2500 });
                    if let Ok(lines) = state.pm.query(&s.id, cmd, tps::RESPONSE_PATTERN, window) {
                        let mut m = sched.servers.lock().unwrap();
                        let e = m.get_mut(&s.id).unwrap();
                        match tps::parse(&lines, cmd) {
                            Some(t) => {
                                e.history.push_back(t.clone());
                                while e.history.len() > HISTORY {
                                    e.history.pop_front();
                                }
                                drop(m);
                                let _ = app.emit("pcm://tps", TpsEvent { id: &s.id, tps: &t });
                            }
                            None => {
                                if tps::is_unknown(&lines) || lines.is_empty() {
                                    e.bad_cmds.insert(cmd.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // ---------- плановый рестарт
        if running && s.schedule.restart_enabled {
            let started = now - chrono::Duration::seconds(st.uptime as i64);
            let post = sched.servers.lock().unwrap()[&s.id].postponed_until;
            if let Some(due) = post.or_else(|| schedule::next_restart(&s.schedule, started, now)) {
                let warn = {
                    let mut m = sched.servers.lock().unwrap();
                    let e = m.get_mut(&s.id).unwrap();
                    if e.due != Some(due) {
                        e.due = Some(due);
                        e.warned.clear();
                    }
                    let w = schedule::warning_now(&s.schedule, due, now, &e.warned);
                    if let Some(w) = w {
                        // все бо́льшие предупреждения считаем отправленными (если менеджер включили поздно)
                        for x in &s.schedule.warn_minutes {
                            if *x >= w {
                                e.warned.push(*x);
                            }
                        }
                    }
                    w
                };
                if let Some(w) = warn {
                    let text = schedule::warn_text(&s.schedule, w);
                    let _ = state.pm.send(&s.id, &format!("say {text}"));
                    state.pm.system(&s.id, format!("⏰ Предупреждение игрокам: {text}"));
                }
                if now >= due {
                    if s.schedule.skip_if_players && !st.players.is_empty() {
                        let later = now + chrono::Duration::minutes(30);
                        sched.servers.lock().unwrap().get_mut(&s.id).unwrap().postponed_until = Some(later);
                        state.pm.system(&s.id, "⏰ На сервере есть игроки — плановый рестарт перенесён на 30 минут");
                    } else {
                        state.pm.system(&s.id, "⏰ Плановый рестарт");
                        let _ = state.pm.send(&s.id, "say Перезапуск…");
                        sched.servers.lock().unwrap().get_mut(&s.id).unwrap().postponed_until = None;
                        let app2 = app.clone();
                        let id = s.id.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = crate::commands::restart_blocking(&app2, &id) {
                                app2.state::<AppState>().pm.system(&id, format!("✖ Плановый рестарт не удался: {e}"));
                            }
                        });
                    }
                }
            }
        }

        // ---------- ночной бэкап
        let last_day = sched.servers.lock().unwrap()[&s.id].last_backup_day.clone();
        if schedule::backup_due(&s.schedule, &last_day, now) && pcm_core::backup::find_restic(&state.paths).is_some() {
            sched.servers.lock().unwrap().get_mut(&s.id).unwrap().last_backup_day = now.date_naive().to_string();
            let app2 = app.clone();
            let id = s.id.clone();
            std::thread::spawn(move || {
                let st = app2.state::<AppState>();
                st.pm.system(&id, "🌙 Ночной бэкап…");
                if let Err(e) = crate::commands::backup_blocking(&app2, &id, "nightly") {
                    st.pm.system(&id, format!("✖ Ночной бэкап не удался: {e}"));
                }
            });
        }
    }
}
