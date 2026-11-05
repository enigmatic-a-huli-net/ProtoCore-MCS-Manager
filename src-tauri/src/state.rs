//! Общее состояние приложения и мост событий из ядра в интерфейс.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pcm_core::config::{AppPaths, Config};
use pcm_core::process::{EventSink, ExitInfo, LogLine, ProcessManager, StatusInfo};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    pub paths: AppPaths,
    pub config: Mutex<Config>,
    pub pm: Arc<ProcessManager>,
    /// Серверы, над которыми идёт долгая операция (бэкап, откат) — чтобы не запустить две сразу
    pub busy: Mutex<HashSet<String>>,
    /// Пользователь нажал «Выход» в трее — больше не удерживаем приложение
    pub quitting: Mutex<bool>,
    /// История быстрых падений для защиты от бесконечного перезапуска
    pub crash_log: Mutex<Vec<(String, Instant)>>,
    /// Сколько раз подряд срабатывал автокарантин (сбрасывается при ручном запуске)
    pub quarantine_rounds: Mutex<std::collections::HashMap<String, u32>>,
}

impl AppState {
    pub fn save(&self) -> Result<(), String> {
        self.config.lock().unwrap().save(&self.paths).map_err(|e| e.to_string())
    }
}

/// Отмечает сервер «занятым» на время операции и снимает отметку при выходе из области.
pub struct BusyGuard<'a> {
    state: &'a AppState,
    id: String,
}

impl<'a> BusyGuard<'a> {
    pub fn acquire(state: &'a AppState, id: &str) -> Result<Self, String> {
        let mut b = state.busy.lock().unwrap();
        if !b.insert(id.to_string()) {
            return Err("над этим сервером уже выполняется операция — дождись её окончания".into());
        }
        Ok(Self { state, id: id.to_string() })
    }
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.state.busy.lock().unwrap().remove(&self.id);
    }
}

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    id: &'a str,
    lines: &'a [LogLine],
}

#[derive(Clone, Serialize)]
struct StatusEvent<'a> {
    id: &'a str,
    info: &'a StatusInfo,
}

#[derive(Clone, Serialize)]
struct ExitEvent<'a> {
    id: &'a str,
    info: &'a ExitInfo,
}

/// Передаёт события процесса в окно. Если окна нет (свёрнуто в трей),
/// emit ничего не стоит — строки и так лежат в буфере ядра.
#[derive(Default)]
pub struct TauriSink {
    pub app: OnceLock<AppHandle>,
}

impl EventSink for TauriSink {
    fn log(&self, id: &str, lines: &[LogLine]) {
        if let Some(app) = self.app.get() {
            let _ = app.emit("pcm://log", LogEvent { id, lines });
        }
    }

    fn status(&self, id: &str, info: &StatusInfo) {
        if let Some(app) = self.app.get() {
            let _ = app.emit("pcm://status", StatusEvent { id, info });
        }
    }

    fn exited(&self, id: &str, info: &ExitInfo) {
        let Some(app) = self.app.get() else { return };
        let _ = app.emit("pcm://exited", ExitEvent { id, info });
        if info.crashed {
            on_crash(app.clone(), id.to_string(), info.clone());
        }
    }
}

#[derive(Clone, Serialize)]
struct QuarantineEvent<'a> {
    id: &'a str,
    mods: &'a [pcm_core::mods::MovedMod],
}

/// После падения:
/// 1. автокарантин — если в логе виден виновный мод, он уходит в mods_disabled/ и сервер стартует снова;
/// 2. иначе автоперезапуск (если включён) с защитой от «краш-петли»:
///    3 падения за 10 минут, едва запустившись, — больше не перезапускаем.
fn on_crash(app: AppHandle, id: String, info: ExitInfo) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let Some(server) = state.config.lock().unwrap().server(&id).cloned() else { return };

        // ---------- автокарантин
        if server.mods.auto_quarantine && server.core.supports_mods() {
            let rounds = {
                let mut r = state.quarantine_rounds.lock().unwrap();
                let e = r.entry(id.clone()).or_insert(0);
                if info.reached_running && info.uptime > 300 {
                    *e = 0; // сервер нормально поработал — счётчик сначала
                }
                *e
            };
            if rounds < server.mods.quarantine_max_rounds {
                let console: String = state
                    .pm
                    .log_buffer(&id, 0)
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let since = std::time::SystemTime::now() - Duration::from_secs(info.uptime + 15);
                let (text, _, _) = pcm_core::analyze::gather(&server.dir(), &console, Some(since));
                match pcm_core::analyze::auto_quarantine(&server, &text, info.reached_running) {
                    Ok((_, moved)) if !moved.is_empty() => {
                        *state.quarantine_rounds.lock().unwrap().entry(id.clone()).or_insert(0) += 1;
                        for m in &moved {
                            state.pm.system(&id, format!("🧪 Карантин: {} → mods_disabled ({})", m.name, m.reason));
                        }
                        let _ = app.emit("pcm://quarantine", QuarantineEvent { id: &id, mods: &moved });
                        if server.mods.quarantine_restart {
                            state.pm.system(&id, "↻ Запуск без этих модов через 3 секунды…");
                            std::thread::sleep(Duration::from_secs(3));
                            if !state.pm.is_running(&id) {
                                if let Err(e) = crate::commands::start_blocking(&app, &id, false) {
                                    state.pm.system(&id, format!("✖ Запуск не удался: {e}"));
                                }
                            }
                        }
                        return;
                    }
                    Ok(_) => {}
                    Err(e) => state.pm.system(&id, format!("⚠ Автокарантин: {e}")),
                }
            } else {
                state.pm.system(&id, "⛔ Лимит автокарантина исчерпан — разберись по вкладке «Диагностика»");
            }
        }

        // ---------- автоперезапуск
        if !server.auto_restart {
            return;
        }
        {
            let mut log = state.crash_log.lock().unwrap();
            log.retain(|(_, t)| t.elapsed() < Duration::from_secs(600));
            log.push((id.clone(), Instant::now()));
            let recent = log.iter().filter(|(i, _)| i == &id).count();
            if recent >= 3 && info.uptime < 300 {
                state.pm.system(
                    &id,
                    "⛔ Сервер падает раз за разом — автоперезапуск остановлен. Посмотри вкладку «Диагностика»",
                );
                return;
            }
        }
        state.pm.system(&id, "↻ Автоперезапуск через 10 секунд…");
        std::thread::sleep(Duration::from_secs(10));
        if state.pm.is_running(&id) {
            return;
        }
        if let Err(e) = crate::commands::start_blocking(&app, &id, false) {
            state.pm.system(&id, format!("✖ Автоперезапуск не удался: {e}"));
        }
    });
}
