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

// ===================================================================== обновления в песочнице

#[derive(Serialize)]
pub struct SandboxResult {
    pub ok: bool,
    pub message: String,
    pub installed: Vec<String>,
}

fn test_entry(server: &ServerEntry, real: &std::path::Path, test: &std::path::Path, tid: &str) -> Result<ServerEntry, String> {
    let mut entry = server.clone();
    entry.id = tid.to_string();
    entry.path = test.to_string_lossy().to_string();
    if entry.launch.mode == LaunchMode::Script {
        let p = PathBuf::from(&entry.launch.script);
        if p.is_absolute() {
            let rel = p
                .strip_prefix(real)
                .map_err(|_| "скрипт запуска лежит вне папки сервера — для проверки в копии выбери режим «Собрать команду»".to_string())?;
            entry.launch.script = rel.to_string_lossy().to_string();
        }
    }
    Ok(entry)
}

/// Скачать обновления в тестовую копию, запустить её и только при успешном
/// запуске перенести новые файлы в настоящий сервер. Блокирующая.
pub fn sandbox_update(app: &AppHandle, id: &str, plan: &pcm_core::catalog::Plan, target: pcm_core::catalog::Target) -> Result<SandboxResult, String> {
    let state = app.state::<AppState>();
    let bs = app.state::<BisectState>();
    let server = state.config.lock().unwrap().server(id).cloned().ok_or("сервер не найден")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut r = bs.running.lock().unwrap();
        if r.contains_key(id) {
            return Err("уже идёт автоперебор или другая проверка в копии".into());
        }
        r.insert(id.to_string(), cancel.clone());
    }
    let real = server.dir();
    let test = bisect::test_dir_for(&real);
    let tid = test_id(id);
    let result = (|| -> Result<SandboxResult, String> {
        state.pm.system(id, "🧪 Готовлю тестовую копию для проверки обновлений…");
        bisect::prepare_copy(&real, &test).map_err(|e| e.to_string())?;
        let replaced: BTreeSet<String> = plan.items.iter().filter_map(|i| i.replaces.clone()).collect();
        let mut files: BTreeSet<String> = pcm_core::util::list_jars(&real.join("mods"))
            .iter()
            .map(|p| pcm_core::util::file_name(p))
            .filter(|n| !replaced.contains(n))
            .collect();
        bisect::set_mods(&real, &test, &files).map_err(|e| e.to_string())?;
        let mut tt = target.clone();
        tt.dir = test.join("mods");
        tt.old_dir = test.join("mods_old");
        let mut last = Instant::now();
        let new_files = pcm_core::catalog::apply(&tt, plan, |title, done, total| {
            if last.elapsed() > Duration::from_millis(200) || done == total {
                last = Instant::now();
                let _ = app.emit("pcm://catalog", crate::commands::CatalogEvent { id: id.to_string(), title: title.into(), done, total });
            }
        })
        .map_err(|e| e.to_string())?;
        files.extend(new_files.iter().cloned());
        state.pm.system(id, format!("🧪 Пробный запуск копии с {} обновлениями (консоль — во вкладке «Диагностика» → автоперебор)", new_files.len()));
        let mut runner = TauriRunner {
            app: app.clone(),
            id: id.to_string(),
            tid: tid.clone(),
            entry: test_entry(&server, &real, &test, &tid)?,
            real: real.clone(),
            test: test.clone(),
            timeout: Duration::from_secs(600),
            cancel: cancel.clone(),
        };
        match runner.run("Проверка обновлений", &files) {
            Outcome::Ok => {}
            Outcome::Crash(why) => return Ok(SandboxResult { ok: false, message: format!("копия с обновлениями упала: {why}. Настоящий сервер не тронут."), installed: vec![] }),
            Outcome::Hang => return Ok(SandboxResult { ok: false, message: "копия с обновлениями не запустилась за 10 минут. Настоящий сервер не тронут.".into(), installed: vec![] }),
            Outcome::Error(e) => return Err(e),
        }
        // успех — переносим проверенные файлы
        std::fs::create_dir_all(&target.dir).map_err(|e| e.to_string())?;
        for f in &new_files {
            std::fs::copy(test.join("mods").join(f), target.dir.join(f)).map_err(|e| e.to_string())?;
        }
        for old in &replaced {
            let p = target.dir.join(old);
            if p.is_file() && !new_files.contains(old) {
                pcm_core::util::move_file(&p, &target.old_dir).map_err(|e| e.to_string())?;
            }
        }
        Ok(SandboxResult { ok: true, message: format!("копия запустилась — установлено обновлений: {}", new_files.len()), installed: new_files })
    })();
    let _ = std::fs::remove_dir_all(&test);
    bs.running.lock().unwrap().remove(id);
    result
}
