//! Запуск автоперебора: тестовый сервер через тот же менеджер процессов (id «<сервер>~test»),
//! поэтому его консоль можно смотреть прямо во вкладке диагностики.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pcm_core::bisect::{self, BisectSettings, Outcome, Report, Runner, Step};
use pcm_core::config::{LaunchMode, ServerEntry};
use pcm_core::process::Status;
use pcm_core::{analyze, launch, mods};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

#[derive(Default)]
pub struct BisectState {
    pub running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub last: Mutex<HashMap<String, Report>>,
}

pub fn test_id(id: &str) -> String {
    format!("{id}~test")
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Ev<'a> {
    Step { id: &'a str, step: &'a Step },
    Status { id: &'a str, text: String },
    Done { id: &'a str, report: &'a Report },
}

struct TauriRunner {
    app: AppHandle,
    id: String,
    tid: String,
    entry: ServerEntry,
    real: PathBuf,
    test: PathBuf,
    timeout: Duration,
    cancel: Arc<AtomicBool>,
}

impl TauriRunner {
    fn reason(&self) -> String {
        let pm = &self.app.state::<AppState>().pm;
        let text: String = pm.log_buffer(&self.tid, 0).iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        let idx = analyze::ModIndex::new(None);
        let f = analyze::analyze_text(&text, &idx);
        // конкретная причина важнее общей «Краш-репорт: …»
        if let Some(x) = f.iter().find(|x| x.code != "crash_report" && x.code != "tick_crash").or(f.first()) {
            return x.title.clone();
        }
        // последняя строка с ошибкой
        pm.log_buffer(&self.tid, 0)
            .iter()
            .rev()
            .find(|l| l.level == pcm_core::process::Level::Error)
            .map(|l| l.text.chars().take(200).collect())
            .unwrap_or_else(|| "сервер упал при запуске".into())
    }
}

impl Runner for TauriRunner {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn run(&mut self, label: &str, files: &BTreeSet<String>) -> Outcome {
        let state = self.app.state::<AppState>();
        let pm = &state.pm;
        if let Err(e) = bisect::set_mods(&self.real, &self.test, files) {
            return Outcome::Error(e.to_string());
        }
        let _ = self.app.emit("pcm://bisect", Ev::Status { id: &self.id, text: format!("{label} · {} модов", files.len()) });
        pm.clear_log(&self.tid);
        pm.system(&self.tid, format!("🔬 {label}: {} модов", files.len()));
        let cmd = match launch::build(&self.entry) {
            Ok(c) => c,
            Err(e) => return Outcome::Error(e.to_string()),
        };
        if let Err(e) = pm.start(&self.tid, &cmd) {
            return Outcome::Error(e.to_string());
        }
        let t0 = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let st = pm.status(&self.tid).status;
            match st {
                Status::Running => {
                    let _ = pm.stop_and_wait(&self.tid, Duration::from_secs(40));
                    return Outcome::Ok;
                }
                Status::Crashed | Status::Stopped => return Outcome::Crash(self.reason()),
                _ => {}
            }
            if self.cancelled() {
                let _ = pm.kill(&self.tid);
                std::thread::sleep(Duration::from_secs(1));
                return Outcome::Error("остановлено".into());
            }
            if t0.elapsed() > self.timeout {
                pm.system(&self.tid, "⏱ Не запустился вовремя — считаю, что завис");
                let _ = pm.kill(&self.tid);
                std::thread::sleep(Duration::from_secs(2));
                return Outcome::Hang;
            }
        }
    }
}

pub fn start(app: AppHandle, id: String, settings: BisectSettings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let bs = app.state::<BisectState>();
    let server = state.config.lock().unwrap().server(&id).cloned().ok_or("сервер не найден")?;
    if !server.core.supports_mods() {
        return Err("это ядро не поддерживает моды".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut r = bs.running.lock().unwrap();
        if r.contains_key(&id) {
            return Err("перебор уже идёт".into());
        }
        r.insert(id.clone(), cancel.clone());
    }

    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let real = server.dir();
        let test = bisect::test_dir_for(&real);
        let emit_status = |t: String| {
            let _ = app.emit("pcm://bisect", Ev::Status { id: &id, text: t });
        };
        let result = (|| -> Result<Report, String> {
            emit_status("Готовлю тестовую копию…".into());
            bisect::prepare_copy(&real, &test).map_err(|e| e.to_string())?;
            let mut entry = server.clone();
            entry.id = test_id(&id);
            entry.path = test.to_string_lossy().to_string();
            entry.launch.memory_mb = settings.memory_mb.max(1024);
            if entry.launch.mode == LaunchMode::Script {
                let p = PathBuf::from(&entry.launch.script);
                if p.is_absolute() {
                    let rel = p.strip_prefix(&real).map_err(|_| "скрипт запуска лежит вне папки сервера — для перебора выбери режим «Собрать команду»".to_string())?;
                    entry.launch.script = rel.to_string_lossy().to_string();
                }
            }
            let report = mods::list(&real, server.core.mod_loader(), &server.mods);
            let g = bisect::Graph::from_report(&report);
            emit_status(format!("Модов: {}. Начинаю…", g.all.len()));
            let mut runner = TauriRunner {
                app: app.clone(),
                id: id.clone(),
                tid: test_id(&id),
                entry,
                real: real.clone(),
                test: test.clone(),
                timeout: Duration::from_secs(settings.timeout_sec.max(60) as u64),
                cancel: cancel.clone(),
            };
            let rep = bisect::run(&g, &settings, &mut runner, &mut |s| {
                let _ = app.emit("pcm://bisect", Ev::Step { id: &id, step: s });
            });
            Ok(rep)
        })();
        let rep = result.unwrap_or_else(|e| Report { message: e, ..Default::default() });
        if !settings.keep_copy {
            let _ = std::fs::remove_dir_all(&test);
        }
        state.pm.system(&id, format!("🔬 Автоперебор: {}", rep.message));
        app.state::<BisectState>().last.lock().unwrap().insert(id.clone(), rep.clone());
        app.state::<BisectState>().running.lock().unwrap().remove(&id);
        let _ = app.emit("pcm://bisect", Ev::Done { id: &id, report: &rep });
    });
    Ok(())
}
