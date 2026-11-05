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

/// Автоперезапуск при падении с защитой от «краш-петли»:
/// если сервер упал 3 раза за 10 минут, едва запустившись, — больше не перезапускаем.
fn on_crash(app: AppHandle, id: String, info: ExitInfo) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let auto = state
            .config
            .lock()
            .unwrap()
            .server(&id)
            .map(|s| s.auto_restart)
            .unwrap_or(false);
        if !auto {
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
                    "⛔ Сервер падает раз за разом — автоперезапуск остановлен. Посмотри «Сжатый лог» и моды",
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
