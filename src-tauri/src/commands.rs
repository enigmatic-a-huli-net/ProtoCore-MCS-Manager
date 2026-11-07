//! Команды, которые вызывает интерфейс (`invoke("имя", {...})`).
//! Тяжёлая работа выполняется в фоновом потоке, чтобы окно не подвисало.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use pcm_core::backup::{self, BackupResult, Progress, Repo, ResticStatus, Snapshot};
use pcm_core::config::{AppPaths, Config, ServerEntry, Theme};
use pcm_core::detect::{self, Detection};
use pcm_core::mods::{self, ModReport, MovedMod};
use pcm_core::plugins::{self, PluginReport};
use pcm_core::process::{LogLine, Metrics, StatusInfo};
use pcm_core::{launch, props, update, util};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::{AppState, BusyGuard};

type R<T> = Result<T, String>;

fn e<E: ToString>(err: E) -> String {
    err.to_string()
}

/// Запускает замыкание в фоне и ждёт результат, не блокируя окно.
async fn bg<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(e)?
}

fn server_of(state: &AppState, id: &str) -> R<ServerEntry> {
    state
        .config
        .lock()
        .unwrap()
        .server(id)
        .cloned()
        .ok_or_else(|| format!("сервер не найден: {id}"))
}

// ===================================================================== общее состояние

#[derive(Serialize)]
pub struct Snapshot0 {
    config: Config,
    paths: AppPaths,
    statuses: HashMap<String, StatusInfo>,
    restic: ResticStatus,
    version: &'static str,
    os: &'static str,
}

#[tauri::command]
pub async fn get_state(app: AppHandle) -> R<Snapshot0> {
    bg(move || {
        let state = app.state::<AppState>();
        let config = state.config.lock().unwrap().clone();
        let statuses = config.servers.iter().map(|s| (s.id.clone(), state.pm.status(&s.id))).collect();
        Ok(Snapshot0 {
            restic: backup::status(&state.paths),
            paths: state.paths.clone(),
            statuses,
            config,
            version: env!("CARGO_PKG_VERSION"),
            os: std::env::consts::OS,
        })
    })
    .await
}

#[derive(Deserialize)]
pub struct AppSettings {
    language: String,
    theme: Theme,
    close_to_tray: bool,
    console_lines: usize,
    check_updates: bool,
    #[serde(default)]
    curseforge_key: String,
    #[serde(default)]
    servers_root: String,
    #[serde(default = "yes")]
    network_monitor: bool,
    #[serde(default)]
    notify: Option<pcm_core::notify::NotifySettings>,
}

fn yes() -> bool {
    true
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: AppSettings) -> R<()> {
    {
        let mut c = state.config.lock().unwrap();
        c.language = settings.language;
        c.theme = settings.theme;
        c.close_to_tray = settings.close_to_tray;
        c.console_lines = settings.console_lines.clamp(500, 50_000);
        c.check_updates = settings.check_updates;
        c.curseforge_key = catalog_key(&settings.curseforge_key);
        c.servers_root = settings.servers_root.trim().to_string();
        c.network_monitor = settings.network_monitor;
        if let Some(mut n) = settings.notify {
            n.telegram_token = n.telegram_token.trim().to_string();
            n.telegram_chat = n.telegram_chat.trim().to_string();
            n.discord_webhook = n.discord_webhook.trim().to_string();
            c.notify = n;
        }
    }
    state.save()
}

#[derive(Serialize)]
pub struct Branding {
    logo: Option<String>,
    splash: Option<String>,
}

/// Свой логотип и GIF загрузки: файлы branding/logo.* и branding/splash.* рядом с .exe.
#[tauri::command]
pub async fn get_branding(app: AppHandle) -> R<Branding> {
    bg(move || {
        let dir = app.state::<AppState>().paths.branding.clone();
        let find = |stem: &str| -> Option<String> {
            ["gif", "webp", "png", "svg", "jpg", "jpeg"]
                .iter()
                .map(|ext| dir.join(format!("{stem}.{ext}")))
                .find(|p| p.is_file())
                .and_then(|p| util::file_to_data_uri(&p))
        };
        Ok(Branding { logo: find("logo"), splash: find("splash") })
    })
    .await
}

// ===================================================================== серверы

#[tauri::command]
pub async fn detect_folder(path: String) -> R<Detection> {
    bg(move || Ok(detect::detect(&PathBuf::from(path)))).await
}

fn apply_detection(s: &mut ServerEntry, d: &Detection) {
    if !s.core_override {
        s.core = d.core;
    }
    if !d.mc_version.is_empty() {
        s.mc_version = d.mc_version.clone();
    }
    s.loader_version = d.loader_version.clone();
}

#[tauri::command]
pub async fn add_server(app: AppHandle, path: String, name: Option<String>) -> R<ServerEntry> {
    bg(move || {
        let state = app.state::<AppState>();
        let dir = PathBuf::from(&path);
        if !dir.is_dir() {
            return Err("папка не найдена".into());
        }
        {
            let c = state.config.lock().unwrap();
            if c.servers.iter().any(|s| PathBuf::from(&s.path) == dir) {
                return Err("этот сервер уже в списке".into());
            }
        }
        let d = detect::detect(&dir);
        let mut s = ServerEntry {
            id: util::short_id(),
            name: name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| util::file_name(&dir)),
            path,
            ..Default::default()
        };
        s.backup.password = util::random_password();
        apply_detection(&mut s, &d);
        // нашли run.bat и нет jar — по умолчанию запускаем через скрипт
        if matches!(d.target, detect::LaunchTarget::None) {
            if let Some(script) = d.scripts.iter().find(|x| x.ends_with(".bat") || x.ends_with(".sh")) {
                s.launch.mode = pcm_core::config::LaunchMode::Script;
                s.launch.script = script.clone();
            }
        }
        {
            let mut c = state.config.lock().unwrap();
            c.selected = s.id.clone();
            c.servers.push(s.clone());
        }
        state.save()?;
        Ok(s)
    })
    .await
}

#[tauri::command]
pub fn remove_server(state: State<AppState>, id: String) -> R<()> {
    if state.pm.is_running(&id) {
        return Err("сначала останови сервер".into());
    }
    {
        let mut c = state.config.lock().unwrap();
        c.servers.retain(|s| s.id != id);
        if c.selected == id {
            c.selected = c.servers.first().map(|s| s.id.clone()).unwrap_or_default();
        }
    }
    // файлы сервера и снапшоты не удаляются — только запись из списка
    state.save()
}

#[tauri::command]
pub fn select_server(state: State<AppState>, id: String) -> R<()> {
    state.config.lock().unwrap().selected = id;
    state.save()
}

#[tauri::command]
pub fn reorder_servers(state: State<AppState>, ids: Vec<String>) -> R<()> {
    {
        let mut c = state.config.lock().unwrap();
        c.servers.sort_by_key(|s| ids.iter().position(|i| i == &s.id).unwrap_or(usize::MAX));
    }
    state.save()
}

/// Сохранить настройки сервера целиком (из формы в интерфейсе).
#[tauri::command]
pub fn update_server(state: State<AppState>, server: ServerEntry) -> R<ServerEntry> {
    let mut c = state.config.lock().unwrap();
    let s = c.server_mut(&server.id).ok_or("сервер не найден")?;
    let password = s.backup.password.clone();
    *s = server;
    if s.backup.password.is_empty() {
        s.backup.password = password; // пароль из интерфейса не приходит пустым по ошибке
    }
    let out = s.clone();
    drop(c);
    state.save()?;
    Ok(out)
}

#[tauri::command]
pub async fn redetect_server(app: AppHandle, id: String) -> R<(ServerEntry, Detection)> {
    bg(move || {
        let state = app.state::<AppState>();
        let mut s = server_of(&state, &id)?;
        let d = detect::detect(&s.dir());
        apply_detection(&mut s, &d);
        {
            let mut c = state.config.lock().unwrap();
            if let Some(x) = c.server_mut(&id) {
                *x = s.clone();
            }
        }
        state.save()?;
        Ok((s, d))
    })
    .await
}

#[tauri::command]
pub fn launch_preview(state: State<AppState>, id: String) -> R<String> {
    let s = server_of(&state, &id)?;
    launch::build(&s).map(|c| c.display).map_err(e)
}

#[tauri::command]
pub fn accept_eula(state: State<AppState>, id: String) -> R<()> {
    let s = server_of(&state, &id)?;
    std::fs::write(
        s.dir().join("eula.txt"),
        "# https://aka.ms/MinecraftEULA\n# принято через ProtoCore-MCS-Manager\neula=true\n",
    )
    .map_err(e)
}

#[tauri::command]
pub fn read_properties(state: State<AppState>, id: String) -> R<Vec<(String, String)>> {
    let s = server_of(&state, &id)?;
    Ok(props::read(&s.dir()))
}

#[tauri::command]
pub fn write_properties(state: State<AppState>, id: String, changes: Vec<(String, String)>) -> R<()> {
    let s = server_of(&state, &id)?;
    props::write(&s.dir(), &changes).map_err(e)
}

// ===================================================================== процесс

#[derive(Clone, Serialize)]
struct BackupEvent {
    id: String,
    phase: &'static str,
    progress: Option<Progress>,
    message: String,
}

fn emit_backup(app: &AppHandle, id: &str, phase: &'static str, progress: Option<Progress>, message: String) {
    let _ = app.emit("pcm://backup", BackupEvent { id: id.into(), phase, progress, message });
}

/// Снапшот с событиями прогресса для интерфейса.
fn snapshot_with_events(app: &AppHandle, server: &ServerEntry, tag: &str) -> R<BackupResult> {
    let state = app.state::<AppState>();
    let repo = Repo::for_server(&state.paths, server).map_err(e)?;
    emit_backup(app, &server.id, "start", None, tag.into());
    let mut last = std::time::Instant::now();
    let res = backup::backup_safely(&state.pm, &repo, server, &["pcm", tag], |p| {
        if last.elapsed() > Duration::from_millis(250) {
            emit_backup(app, &server.id, "progress", Some(p), String::new());
            last = std::time::Instant::now();
        }
    });
    match &res {
        Ok(r) => {
            emit_backup(app, &server.id, "done", None, r.snapshot_id.clone());
            // чистка старых снапшотов — в фоне, она может занять время
            let keep = server.backup.keep_last;
            std::thread::spawn(move || {
                let _ = repo.forget(keep);
            });
        }
        Err(err) => {
            emit_backup(app, &server.id, "error", None, err.to_string());
            if app.state::<AppState>().config.lock().unwrap().notify.on_backup_fail {
                crate::social::notify(app, &format!("backup:{}", server.id), Duration::from_secs(600), format!("⚠ Снапшот «{}» не удался: {err}", server.name));
            }
        }
    }
    res.map_err(e)
}

/// Запуск без проверки «занятости» (для вызова изнутри других операций).
fn start_inner(app: &AppHandle, id: &str, allow_snapshot: bool) -> R<()> {
    let state = app.state::<AppState>();
    let s = server_of(&state, id)?;
    if state.pm.is_running(id) {
        return Err("сервер уже запущен".into());
    }
    let d = detect::detect(&s.dir());
    if !d.eula_accepted && s.core != detect::CoreKind::Unknown {
        return Err("EULA_REQUIRED".into());
    }

    // 1. убрать клиентские моды
    if s.mods.auto_disable_client && s.core.supports_mods() && s.dir().join(mods::ENABLED_DIR).is_dir() {
        match mods::disable_client_mods(&s.dir(), s.core.mod_loader(), &s.mods, s.mods.disable_blacklisted) {
            Ok(moved) if !moved.is_empty() => {
                for m in &moved {
                    state.pm.system(id, format!("🧹 Клиентский мод отключён: {} ({})", m.name, m.reason));
                }
            }
            Ok(_) => {}
            Err(err) => state.pm.system(id, format!("⚠ Не удалось проверить клиентские моды: {err}")),
        }
    }

    // 2. снапшот перед запуском
    if allow_snapshot && s.backup.snapshot_on_start {
        if backup::find_restic(&state.paths).is_none() {
            state.pm.system(id, "⚠ Снапшот пропущен: restic не скачан (вкладка «Бэкапы»)");
        } else {
            state.pm.system(id, "💾 Снапшот перед запуском…");
            match snapshot_with_events(app, &s, "on-start") {
                Ok(r) => state.pm.system(
                    id,
                    format!(
                        "💾 Снапшот {} готов: добавлено {} за {:.1} с",
                        &r.snapshot_id[..8.min(r.snapshot_id.len())],
                        backup::human_bytes(r.data_added),
                        r.seconds
                    ),
                ),
                Err(err) => state.pm.system(id, format!("⚠ Снапшот не удался, запускаю без него: {err}")),
            }
        }
    }

    // 3. запуск
    let cmd = launch::build(&s).map_err(e)?;
    state.pm.start(id, &cmd).map_err(e)
}

pub fn start_blocking(app: &AppHandle, id: &str, allow_snapshot: bool) -> R<()> {
    let state = app.state::<AppState>();
    let _g = BusyGuard::acquire(&state, id)?;
    start_inner(app, id, allow_snapshot)
}

#[tauri::command]
pub async fn start_server(app: AppHandle, id: String) -> R<()> {
    bg(move || {
        app.state::<AppState>().quarantine_rounds.lock().unwrap().remove(&id);
        start_blocking(&app, &id, true)
    })
    .await
}

#[tauri::command]
pub async fn stop_server(app: AppHandle, id: String) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let t = server_of(&state, &id)?.launch.stop_timeout_sec.max(10);
        state.pm.stop(&id, Duration::from_secs(t as u64)).map_err(e)
    })
    .await
}

#[tauri::command]
pub fn kill_server(state: State<AppState>, id: String) -> R<()> {
    state.pm.kill(&id).map_err(e)
}

pub fn restart_blocking(app: &AppHandle, id: &str) -> R<()> {
    let state = app.state::<AppState>();
    let _g = BusyGuard::acquire(&state, id)?;
    let t = server_of(&state, id)?.launch.stop_timeout_sec.max(10);
    state.pm.system(id, "↻ Перезапуск…");
    state.pm.stop_and_wait(id, Duration::from_secs(t as u64)).map_err(e)?;
    start_inner(app, id, true)
}

#[tauri::command]
pub async fn restart_server(app: AppHandle, id: String) -> R<()> {
    bg(move || restart_blocking(&app, &id)).await
}

pub fn backup_blocking(app: &AppHandle, id: &str, tag: &str) -> R<BackupResult> {
    let state = app.state::<AppState>();
    let _g = BusyGuard::acquire(&state, id)?;
    let s = server_of(&state, id)?;
    snapshot_with_events(app, &s, tag)
}

#[tauri::command]
pub fn get_monitor(app: AppHandle, id: String) -> crate::scheduler::MonitorInfo {
    crate::scheduler::info(&app, &id)
}

#[tauri::command]
pub fn send_command(app: AppHandle, state: State<AppState>, id: String, command: String) -> R<()> {
    state.pm.system(&id, format!("> {}", command.trim()));
    crate::social::audit(&app, &id, command.trim(), "console");
    state.pm.send(&id, &command).map_err(e)
}

#[tauri::command]
pub fn get_log(state: State<AppState>, id: String, after: u64) -> Vec<LogLine> {
    state.pm.log_buffer(&id, after)
}

#[tauri::command]
pub fn clear_log(state: State<AppState>, id: String) {
    state.pm.clear_log(&id)
}

#[tauri::command]
pub async fn get_metrics(app: AppHandle, id: String) -> R<Metrics> {
    bg(move || Ok(app.state::<AppState>().pm.metrics(&id))).await
}

// ===================================================================== моды и плагины

#[tauri::command]
pub async fn list_mods(app: AppHandle, id: String) -> R<ModReport> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        Ok(mods::list(&s.dir(), s.core.mod_loader(), &s.mods))
    })
    .await
}

#[tauri::command]
pub async fn set_mod_enabled(app: AppHandle, id: String, file: String, enabled: bool) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        mods::set_enabled(&s.dir(), &file, enabled, "user", "").map(|_| ()).map_err(e)
    })
    .await
}

#[tauri::command]
pub async fn set_mods_enabled(app: AppHandle, id: String, files: Vec<String>, enabled: bool) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        for f in files {
            mods::set_enabled(&s.dir(), &f, enabled, "user", "").map_err(e)?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn disable_client_mods(app: AppHandle, id: String, include_likely: bool) -> R<Vec<MovedMod>> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        mods::disable_client_mods(&s.dir(), s.core.mod_loader(), &s.mods, include_likely).map_err(e)
    })
    .await
}

#[tauri::command]
pub async fn list_plugins(app: AppHandle, id: String) -> R<PluginReport> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        Ok(plugins::list(&s.dir()))
    })
    .await
}

#[tauri::command]
pub async fn set_plugin_enabled(app: AppHandle, id: String, file: String, enabled: bool) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        plugins::set_enabled(&s.dir(), &file, enabled, "user").map(|_| ()).map_err(e)
    })
    .await
}

// ===================================================================== бэкапы

#[tauri::command]
pub async fn download_restic(app: AppHandle) -> R<ResticStatus> {
    bg(move || {
        let state = app.state::<AppState>();
        backup::download_restic(&state.paths).map_err(e)?;
        Ok(backup::status(&state.paths))
    })
    .await
}

#[tauri::command]
pub async fn restic_status(app: AppHandle) -> R<ResticStatus> {
    bg(move || Ok(backup::status(&app.state::<AppState>().paths))).await
}

#[derive(Serialize)]
pub struct SnapshotList {
    snapshots: Vec<Snapshot>,
    repo_path: String,
    repo_size: u64,
}

#[tauri::command]
pub async fn list_snapshots(app: AppHandle, id: String) -> R<SnapshotList> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let repo_path = s.restic_repo(&state.paths).to_string_lossy().to_string();
        match Repo::for_server(&state.paths, &s) {
            Ok(repo) => Ok(SnapshotList { snapshots: repo.snapshots().map_err(e)?, repo_size: repo.size_on_disk(), repo_path }),
            Err(_) => Ok(SnapshotList { snapshots: vec![], repo_size: 0, repo_path }),
        }
    })
    .await
}

#[tauri::command]
pub async fn backup_now(app: AppHandle, id: String) -> R<BackupResult> {
    bg(move || {
        backup_blocking(&app, &id, "manual")
    })
    .await
}

#[tauri::command]
pub async fn delete_snapshot(app: AppHandle, id: String, snapshot: String) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let _g = BusyGuard::acquire(&state, &id)?;
        let s = server_of(&state, &id)?;
        Repo::for_server(&state.paths, &s).map_err(e)?.delete_snapshot(&snapshot).map_err(e)
    })
    .await
}

/// Откат одной кнопкой: остановка → снапшот текущего состояния → восстановление → запуск.
#[tauri::command]
pub async fn rollback(app: AppHandle, id: String, snapshot: String, start_after: bool) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let _g = BusyGuard::acquire(&state, &id)?;
        let s = server_of(&state, &id)?;
        let repo = Repo::for_server(&state.paths, &s).map_err(e)?;
        let snap = repo
            .snapshots()
            .map_err(e)?
            .into_iter()
            .find(|x| x.id == snapshot || x.short_id == snapshot)
            .ok_or("снапшот не найден")?;

        let was_running = state.pm.is_running(&id);
        if was_running {
            state.pm.system(&id, "⏪ Откат: останавливаю сервер…");
            state
                .pm
                .stop_and_wait(&id, Duration::from_secs(s.launch.stop_timeout_sec.max(10) as u64))
                .map_err(e)?;
        }
        state.pm.system(&id, "⏪ Откат: снапшот текущего состояния (на всякий случай)…");
        snapshot_with_events(&app, &s, "pre-rollback")?;
        state.pm.system(&id, format!("⏪ Откат: восстанавливаю {} от {}…", snap.short_id, snap.time));
        emit_backup(&app, &id, "restore", None, snap.short_id.clone());
        repo.restore(&snap, &s.dir(), &s.backup.excludes).map_err(e)?;
        emit_backup(&app, &id, "restored", None, snap.short_id.clone());
        state.pm.system(&id, "✔ Откат завершён");
        if start_after || was_running {
            // свежий снапшот только что сделан — повторный при запуске не нужен
            start_inner(&app, &id, false)?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn check_repo(app: AppHandle, id: String) -> R<String> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        Repo::for_server(&state.paths, &s).map_err(e)?.check().map_err(e)
    })
    .await
}

// ===================================================================== обновление менеджера

#[tauri::command]
pub async fn check_update(app: AppHandle) -> R<update::UpdateInfo> {
    bg(move || {
        let repo = app.state::<AppState>().config.lock().unwrap().update_repo.clone();
        update::check(&repo, env!("CARGO_PKG_VERSION")).map_err(e)
    })
    .await
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    done: u64,
    total: u64,
}

/// Скачать, подготовить замену и закрыть менеджер (серверы мягко останавливаются).
#[tauri::command]
pub async fn install_update(app: AppHandle) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let repo = state.config.lock().unwrap().update_repo.clone();
        let info = update::check(&repo, env!("CARGO_PKG_VERSION")).map_err(e)?;
        if !info.available {
            return Err("уже установлена последняя версия".into());
        }
        let mut last = std::time::Instant::now();
        update::download(&info, |done, total| {
            if last.elapsed() > Duration::from_millis(150) || done == total {
                let _ = app.emit("pcm://update", UpdateProgress { done, total });
                last = std::time::Instant::now();
            }
        })
        .map_err(e)?;
        update::apply(&state.paths).map_err(e)?;
        crate::quit(&app);
        Ok(())
    })
    .await
}

// ===================================================================== диагностика

/// source: "auto" (свежий краш-репорт + консоль), "crash", "latest" (logs/latest.log), "console"
#[tauri::command]
pub async fn analyze_logs(app: AppHandle, id: String, source: String) -> R<pcm_core::analyze::Analysis> {
    bg(move || {
        use pcm_core::analyze;
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let dir = s.dir();
        let console: String = state.pm.log_buffer(&id, 0).iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        let (text, src, path) = match source.as_str() {
            "crash" => {
                let p = analyze::latest_crash_report(&dir).ok_or("краш-репортов нет")?;
                (std::fs::read_to_string(&p).map_err(e)?, "crash".to_string(), p.to_string_lossy().to_string())
            }
            "latest" => {
                let p = dir.join("logs/latest.log");
                (std::fs::read_to_string(&p).map_err(|_| "logs/latest.log не найден".to_string())?, "latest".into(), p.to_string_lossy().to_string())
            }
            "console" => (console, "console".into(), String::new()),
            _ => {
                // авто: краш-репорт за последние сутки + консоль; если консоль пустая — latest.log
                let day = std::time::SystemTime::now() - Duration::from_secs(86_400);
                let base = if console.trim().is_empty() {
                    std::fs::read_to_string(dir.join("logs/latest.log")).unwrap_or_default()
                } else {
                    console
                };
                analyze::gather(&dir, &base, Some(day))
            }
        };
        if text.trim().is_empty() {
            return Err("лог пуст — запусти сервер или выбери другой источник".into());
        }
        let report = s.core.supports_mods().then(|| mods::list(&dir, s.core.mod_loader(), &s.mods));
        let header = format!(
            "== ProtoCore-MCS-Manager {} · {} {} · Minecraft {} · источник: {} ==",
            env!("CARGO_PKG_VERSION"),
            s.core.label(),
            s.loader_version,
            s.mc_version,
            if path.is_empty() { src.clone() } else { path.clone() }
        );
        Ok(analyze::run(&src, &path, &text, report.as_ref(), &header))
    })
    .await
}

/// Сохраняет сжатый лог в папку сервера и возвращает путь.
#[tauri::command]
pub fn save_compressed(state: State<AppState>, id: String, text: String) -> R<String> {
    let s = server_of(&state, &id)?;
    let dir = s.dir().join("pcm-logs");
    std::fs::create_dir_all(&dir).map_err(e)?;
    let name = format!("compact-{}.txt", util::stamp());
    let p = dir.join(name);
    std::fs::write(&p, text).map_err(e)?;
    Ok(p.to_string_lossy().to_string())
}

/// Отправить мод в карантин вручную (из диагностики).
#[tauri::command]
pub async fn quarantine_mod(app: AppHandle, id: String, file: String, reason: String) -> R<()> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        mods::set_enabled(&s.dir(), &file, false, "crash", &reason).map(|_| ()).map_err(e)
    })
    .await
}

// ===================================================================== ядра и новые серверы

use pcm_core::catalog::{self, Kind, Plan, Source};
use pcm_core::cores;
use pcm_core::detect::CoreKind;

#[tauri::command]
pub async fn core_versions(core: CoreKind) -> R<Vec<cores::McVersion>> {
    bg(move || cores::mc_versions(core).map_err(e)).await
}

#[tauri::command]
pub async fn core_builds(core: CoreKind, mc: String) -> R<Vec<cores::Build>> {
    bg(move || cores::builds(core, &mc).map_err(e)).await
}

#[derive(Clone, Serialize)]
struct CreateEvent {
    /// id сервера, если это обновление ядра; "new" — мастер нового сервера
    id: String,
    line: Option<String>,
    done: u64,
    total: u64,
}

fn create_emitter(app: &AppHandle, id: &str) -> (impl FnMut(String), impl FnMut(u64, u64)) {
    let (a1, a2) = (app.clone(), app.clone());
    let (i1, i2) = (id.to_string(), id.to_string());
    let mut last = std::time::Instant::now();
    (
        move |line: String| {
            let _ = a1.emit("pcm://create", CreateEvent { id: i1.clone(), line: Some(line), done: 0, total: 0 });
        },
        move |done: u64, total: u64| {
            if last.elapsed() > Duration::from_millis(200) || done == total {
                last = std::time::Instant::now();
                let _ = a2.emit("pcm://create", CreateEvent { id: i2.clone(), line: None, done, total });
            }
        },
    )
}

#[derive(Deserialize)]
pub struct NewServer {
    name: String,
    dir: String,
    core: CoreKind,
    mc: String,
    build: String,
    memory_mb: u32,
    java_path: String,
    accept_eula: bool,
}

#[tauri::command]
pub async fn create_server(app: AppHandle, req: NewServer) -> R<ServerEntry> {
    bg(move || {
        let state = app.state::<AppState>();
        let dir = PathBuf::from(req.dir.trim());
        if dir.as_os_str().is_empty() {
            return Err("не указана папка".into());
        }
        if dir.is_dir() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err("папка не пустая — выбери новую или пустую папку".into());
        }
        let java = if req.java_path.trim().is_empty() { "java".to_string() } else { req.java_path.trim().to_string() };
        let (mut log, progress) = create_emitter(&app, "new");
        log(format!("📁 {}", dir.display()));
        let res = cores::install(req.core, &req.mc, &req.build, &dir, &java, &mut log, progress).map_err(e)?;
        cores::write_defaults(&dir, req.core, req.accept_eula, &req.name).map_err(e)?;
        log("✔ Ядро установлено".into());

        let d = detect::detect(&dir);
        let mut s = ServerEntry {
            id: util::short_id(),
            name: if req.name.trim().is_empty() { util::file_name(&dir) } else { req.name.trim().to_string() },
            path: dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        s.backup.password = util::random_password();
        apply_detection(&mut s, &d);
        if s.core == CoreKind::Unknown {
            s.core = req.core;
        }
        if s.mc_version.is_empty() {
            s.mc_version = req.mc.clone();
        }
        if s.loader_version.is_empty() && req.core != CoreKind::Vanilla {
            s.loader_version = res.build.clone();
        }
        s.launch.memory_mb = req.memory_mb.max(1024);
        s.launch.java_path = req.java_path.trim().to_string();
        if let (Some(j), detect::LaunchTarget::None) = (&res.jar, &d.target) {
            s.launch.jar_override = j.clone();
        }
        {
            let mut c = state.config.lock().unwrap();
            c.selected = s.id.clone();
            c.servers.push(s.clone());
        }
        state.save()?;
        Ok(s)
    })
    .await
}

/// Обновить ядро существующего сервера (сервер должен быть остановлен).
#[tauri::command]
pub async fn update_core(app: AppHandle, id: String, build: String) -> R<ServerEntry> {
    bg(move || {
        let state = app.state::<AppState>();
        let _g = BusyGuard::acquire(&state, &id)?;
        if state.pm.is_running(&id) {
            return Err("сначала останови сервер".into());
        }
        let s = server_of(&state, &id)?;
        let (mut log, progress) = create_emitter(&app, &id);
        if backup::find_restic(&state.paths).is_some() {
            log("💾 Снапшот перед обновлением ядра…".into());
            snapshot_with_events(&app, &s, "pre-core-update")?;
        }
        let moved = cores::archive_old_core_jars(&s.dir(), s.core).map_err(e)?;
        for m in &moved {
            log(format!("↪ {} → cores_old", util::file_name(m)));
        }
        let java = if s.launch.java_path.trim().is_empty() { "java".to_string() } else { s.launch.java_path.clone() };
        let res = cores::install(s.core, &s.mc_version, &build, &s.dir(), &java, &mut log, progress).map_err(e)?;
        let mut s2 = s.clone();
        let d = detect::detect(&s2.dir());
        apply_detection(&mut s2, &d);
        s2.loader_version = if d.loader_version.is_empty() { res.build.clone() } else { d.loader_version.clone() };
        if !s2.launch.jar_override.is_empty() {
            s2.launch.jar_override = res.jar.clone().unwrap_or_default();
        }
        {
            let mut c = state.config.lock().unwrap();
            if let Some(x) = c.server_mut(&id) {
                *x = s2.clone();
            }
        }
        state.save()?;
        state.pm.system(&id, format!("⬆ Ядро обновлено: {} {}", s2.core.label(), s2.loader_version));
        log("✔ Готово".into());
        Ok(s2)
    })
    .await
}

#[tauri::command]
pub async fn java_check(java: String) -> R<String> {
    bg(move || cores::java_version(if java.trim().is_empty() { "java" } else { java.trim() }).map_err(e)).await
}

// ===================================================================== каталог

fn target(state: &AppState, id: &str, kind: Kind) -> R<catalog::Target> {
    let s = server_of(state, id)?;
    let key = state.config.lock().unwrap().curseforge_key.clone();
    Ok(catalog::Target::for_server(&s, kind, &key))
}

#[tauri::command]
pub async fn catalog_search(app: AppHandle, id: String, kind: Kind, source: Source, query: String, sort: String, offset: u32) -> R<catalog::SearchResult> {
    bg(move || {
        let t = target(&app.state::<AppState>(), &id, kind)?;
        catalog::search(&t, source, &query, &sort, offset).map_err(e)
    })
    .await
}

#[tauri::command]
pub async fn catalog_versions(app: AppHandle, id: String, kind: Kind, source: Source, project: String) -> R<Vec<catalog::FileRef>> {
    bg(move || {
        let t = target(&app.state::<AppState>(), &id, kind)?;
        catalog::versions(&t, source, &project).map_err(e)
    })
    .await
}

#[tauri::command]
pub async fn catalog_plan(app: AppHandle, id: String, kind: Kind, source: Source, project: String, version: Option<String>) -> R<Plan> {
    bg(move || {
        let t = target(&app.state::<AppState>(), &id, kind)?;
        catalog::plan_install(&t, source, &project, version.as_deref()).map_err(e)
    })
    .await
}

#[derive(Clone, Serialize)]
struct CatalogEvent {
    id: String,
    title: String,
    done: u64,
    total: u64,
}

#[tauri::command]
pub async fn catalog_apply(app: AppHandle, id: String, kind: Kind, plan: Plan) -> R<Vec<String>> {
    bg(move || {
        let state = app.state::<AppState>();
        let t = target(&state, &id, kind)?;
        let mut last = std::time::Instant::now();
        let files = catalog::apply(&t, &plan, |title, done, total| {
            if last.elapsed() > Duration::from_millis(200) || done == total {
                last = std::time::Instant::now();
                let _ = app.emit("pcm://catalog", CatalogEvent { id: id.clone(), title: title.into(), done, total });
            }
        })
        .map_err(e)?;
        for f in &files {
            state.pm.system(&id, format!("⬇ Установлено: {f}"));
        }
        Ok(files)
    })
    .await
}

#[tauri::command]
pub async fn catalog_updates(app: AppHandle, id: String, kind: Kind) -> R<catalog::UpdatesReport> {
    bg(move || {
        let t = target(&app.state::<AppState>(), &id, kind)?;
        catalog::check_updates(&t).map_err(e)
    })
    .await
}

/// «Скачать подходящую версию» для ошибки «не та версия» / «нет мода».
#[tauri::command]
pub async fn catalog_fix(app: AppHandle, id: String, mod_id: String, range: String) -> R<Plan> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let t = target(&state, &id, Kind::Mod)?;
        // файл, который сейчас стоит под этим id (его заменим)
        let report = mods::list(&s.dir(), s.core.mod_loader(), &s.mods);
        let old = report
            .mods
            .iter()
            .find(|m| m.enabled && m.meta.ids.iter().any(|x| x.eq_ignore_ascii_case(&mod_id)));
        let range = if range.trim().is_empty() { "*".to_string() } else { range };
        catalog::plan_fix_version(&t, &mod_id, &range, old.map(|m| m.file.as_str()), old.map(|m| m.meta.name.as_str())).map_err(e)
    })
    .await
}

fn catalog_key(k: &str) -> String {
    pcm_core::catalog::clean_cf_key(k)
}

#[tauri::command]
pub async fn check_cf_key(key: String) -> R<String> {
    bg(move || pcm_core::catalog::check_cf_key(&key).map_err(e)).await
}

// ===================================================================== открыть папку / файл

/// Открыть папку или файл в проводнике. `reveal` — показать файл выделенным в его папке.
/// Делается на стороне ядра: так не нужен белый список путей у плагина открытия.
#[tauri::command]
pub fn open_path(app: AppHandle, path: String, reveal: bool) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let p = PathBuf::from(path.trim());
    // папки mods/plugins могут ещё не существовать — создаём, чтобы было что открыть
    if !reveal && !p.exists() && p.extension().is_none() {
        std::fs::create_dir_all(&p).map_err(e)?;
    }
    if reveal {
        app.opener().reveal_item_in_dir(&p).map_err(e)
    } else {
        app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(e)
    }
}

// ===================================================================== автоперебор

#[tauri::command]
pub fn bisect_start(app: AppHandle, id: String, settings: pcm_core::bisect::BisectSettings) -> R<()> {
    crate::bisect_run::start(app, id, settings)
}

#[tauri::command]
pub fn bisect_cancel(app: AppHandle, id: String) {
    if let Some(c) = app.state::<crate::bisect_run::BisectState>().running.lock().unwrap().get(&id) {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Serialize)]
pub struct BisectInfo {
    running: bool,
    last: Option<pcm_core::bisect::Report>,
    test_id: String,
}

#[tauri::command]
pub fn bisect_info(app: AppHandle, id: String) -> BisectInfo {
    let bs = app.state::<crate::bisect_run::BisectState>();
    let running = bs.running.lock().unwrap().contains_key(&id);
    let last = bs.last.lock().unwrap().get(&id).cloned();
    BisectInfo { running, last, test_id: crate::bisect_run::test_id(&id) }
}

// ===================================================================== советы

#[tauri::command]
pub async fn get_advice(app: AppHandle, id: String) -> R<Vec<pcm_core::advice::Advice>> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let dir = s.dir();
        let report = s.core.supports_mods().then(|| mods::list(&dir, s.core.mod_loader(), &s.mods));
        let plugin_names: Vec<String> = if s.core.supports_plugins() {
            plugins::list(&dir).plugins.into_iter().filter(|p| p.enabled).map(|p| p.name).collect()
        } else {
            vec![]
        };
        let java = if s.launch.java_path.trim().is_empty() { "java".to_string() } else { s.launch.java_path.clone() };
        let java_major = cores::java_version(&java).ok().and_then(|v| pcm_core::advice::java_major(&v));
        let lag = std::fs::read_to_string(dir.join("logs/latest.log"))
            .map(|t| t.lines().filter(|l| l.contains("Can't keep up!")).count())
            .unwrap_or(0);
        let sys_mb = state.pm.metrics(&id).system_ram_total / 1024 / 1024;
        Ok(pcm_core::advice::advise(&pcm_core::advice::Input {
            server: &s,
            mods: report.as_ref(),
            plugin_names,
            props: props::read(&dir),
            system_ram_mb: sys_mb,
            java_major,
            lag_warnings: lag,
        }))
    })
    .await
}

// ===================================================================== сеть

#[tauri::command]
pub fn net_report(app: AppHandle) -> crate::netmon::NetReport {
    crate::netmon::report(&app)
}

fn server_port(state: &AppState, id: &str) -> R<u16> {
    let s = server_of(state, id)?;
    Ok(props::read(&s.dir())
        .iter()
        .find(|(k, _)| k == "server-port")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(25565))
}

#[tauri::command]
pub async fn connect_info(app: AppHandle, id: String, check: bool) -> R<pcm_core::network::ConnectInfo> {
    bg(move || {
        let state = app.state::<AppState>();
        let port = server_port(&state, &id)?;
        // проверка снаружи имеет смысл, только когда сервер слушает порт
        let check = check && state.pm.status(&id).status == pcm_core::process::Status::Running;
        Ok(pcm_core::network::connect_info(port, check))
    })
    .await
}

#[tauri::command]
pub async fn upnp_open(app: AppHandle, id: String) -> R<u16> {
    bg(move || {
        let port = server_port(&app.state::<AppState>(), &id)?;
        pcm_core::network::upnp_open(port).map_err(e)?;
        Ok(port)
    })
    .await
}

#[tauri::command]
pub async fn upnp_close(app: AppHandle, id: String) -> R<()> {
    bg(move || {
        let port = server_port(&app.state::<AppState>(), &id)?;
        pcm_core::network::upnp_close(port).map_err(e)
    })
    .await
}

// ===================================================================== сборки

#[tauri::command]
pub async fn modpack_inspect(path: String) -> R<pcm_core::modpack::PackInfo> {
    bg(move || pcm_core::modpack::inspect(&PathBuf::from(path)).map_err(e)).await
}

#[derive(Deserialize)]
pub struct ImportReq {
    pack: String,
    name: String,
    dir: String,
    memory_mb: u32,
    java_path: String,
    accept_eula: bool,
}

#[derive(Serialize)]
pub struct ImportOut {
    server: ServerEntry,
    result: pcm_core::modpack::ImportResult,
}

/// Новый сервер из сборки: ядро нужной версии + файлы сборки.
#[tauri::command]
pub async fn modpack_import(app: AppHandle, req: ImportReq) -> R<ImportOut> {
    bg(move || {
        use pcm_core::modpack;
        let state = app.state::<AppState>();
        let pack = PathBuf::from(&req.pack);
        let info = modpack::inspect(&pack).map_err(e)?;
        let dir = PathBuf::from(req.dir.trim());
        if dir.is_dir() && std::fs::read_dir(&dir).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err("папка не пустая — выбери новую или пустую папку".into());
        }
        let java = if req.java_path.trim().is_empty() { "java".to_string() } else { req.java_path.trim().to_string() };
        let cf_key = state.config.lock().unwrap().curseforge_key.clone();
        let (mut log, mut progress) = create_emitter(&app, "new");
        log(format!("📦 {} {} — {} {} · Minecraft {}", info.name, info.version, info.core.label(), info.loader_version, info.mc));
        let core_res = cores::install(info.core, &info.mc, &info.loader_version, &dir, &java, &mut log, &mut progress).map_err(e)?;
        log("⬇ Файлы сборки…".into());
        let result = modpack::import_files(&pack, &dir, &cf_key, |i, n, name| {
            progress(i as u64, n as u64);
            if i % 10 == 0 || i == n {
                log(format!("  {i}/{n} {name}"));
            }
        })
        .map_err(e)?;
        log(format!(
            "✔ Скачано {}, пропущено клиентских {}, файлов настроек {}{}",
            result.downloaded,
            result.skipped_client,
            result.overrides,
            if result.manual.is_empty() { String::new() } else { format!(", вручную: {}", result.manual.len()) }
        ));
        cores::write_defaults(&dir, info.core, req.accept_eula, &info.name).map_err(e)?;

        let d = detect::detect(&dir);
        let mut s = ServerEntry {
            id: util::short_id(),
            name: if req.name.trim().is_empty() { info.name.clone() } else { req.name.trim().to_string() },
            path: dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        s.backup.password = util::random_password();
        apply_detection(&mut s, &d);
        if s.core == CoreKind::Unknown {
            s.core = info.core;
        }
        if s.mc_version.is_empty() {
            s.mc_version = info.mc.clone();
        }
        if s.loader_version.is_empty() {
            s.loader_version = core_res.build.clone();
        }
        s.launch.memory_mb = req.memory_mb.max(2048);
        s.launch.java_path = req.java_path.trim().to_string();
        if let (Some(j), detect::LaunchTarget::None) = (&core_res.jar, &d.target) {
            s.launch.jar_override = j.clone();
        }
        {
            let mut c = state.config.lock().unwrap();
            c.selected = s.id.clone();
            c.servers.push(s.clone());
        }
        state.save()?;
        Ok(ImportOut { server: s, result })
    })
    .await
}

/// Версия NeoForge/Forge для клиентской сборки: у гибридов (Youer) её нет в имени ядра.
fn loader_version_for_pack(s: &ServerEntry) -> String {
    match s.core {
        CoreKind::NeoForge | CoreKind::Forge | CoreKind::Fabric | CoreKind::Quilt => s.loader_version.clone(),
        _ => {
            let neo = s.dir().join("libraries/net/neoforged/neoforge");
            let mut v: Vec<String> = std::fs::read_dir(&neo)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            v.sort_by(|a, b| detect::version_cmp(a, b));
            v.pop().or_else(|| {
                let core = if s.core == CoreKind::Mohist { CoreKind::Forge } else { CoreKind::NeoForge };
                cores::builds(core, &s.mc_version).ok().and_then(|b| b.iter().find(|x| x.stable).map(|x| x.id.clone()))
            })
            .unwrap_or_default()
        }
    }
}

#[tauri::command]
pub async fn modpack_export(app: AppHandle, id: String, path: String, include_config: bool) -> R<pcm_core::modpack::ExportResult> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let core = match s.core {
            CoreKind::Youer | CoreKind::Arclight => CoreKind::NeoForge,
            CoreKind::Mohist => CoreKind::Forge,
            c => c,
        };
        let opt = pcm_core::modpack::ExportOptions {
            name: s.name.clone(),
            version: chrono::Local::now().format("%Y.%m.%d").to_string(),
            mc: s.mc_version.clone(),
            core,
            loader_version: loader_version_for_pack(&s),
            include_config,
        };
        pcm_core::modpack::export_client(&s.dir(), &s.mods, &opt, &PathBuf::from(path)).map_err(e)
    })
    .await
}

// ===================================================================== игроки и уведомления

#[derive(Serialize)]
pub struct PlayerRow {
    name: String,
    online: bool,
    #[serde(flatten)]
    rec: pcm_core::players::PlayerRec,
    op: bool,
    whitelisted: bool,
    banned: bool,
}

#[derive(Serialize)]
pub struct PlayersInfo {
    running: bool,
    rows: Vec<PlayerRow>,
    lists: pcm_core::players::ServerLists,
    shared_ips: Vec<(String, Vec<String>)>,
    chat: Vec<pcm_core::players::ChatLine>,
    audit: Vec<pcm_core::players::AuditLine>,
}

#[tauri::command]
pub async fn players_info(app: AppHandle, id: String) -> R<PlayersInfo> {
    bg(move || {
        let state = app.state::<AppState>();
        let s = server_of(&state, &id)?;
        let st = state.pm.status(&id);
        let lists = pcm_core::players::lists(&s.dir());
        let (players, shared, chat, audit) = crate::social::with_db(&app, &id, |db| {
            (
                db.players.clone(),
                db.shared_ips(),
                db.chat.iter().rev().take(200).cloned().collect::<Vec<_>>(),
                db.audit.iter().rev().take(100).cloned().collect::<Vec<_>>(),
            )
        });
        let lower = |v: &[String]| v.iter().map(|x| x.to_lowercase()).collect::<std::collections::HashSet<_>>();
        let ops = lower(&lists.ops);
        let wl = lower(&lists.whitelist);
        let bans: std::collections::HashSet<String> = lists.banned.iter().map(|x| x.0.to_lowercase()).collect();
        let mut names: std::collections::BTreeSet<String> = players.keys().cloned().collect();
        names.extend(lists.known.keys().cloned());
        names.extend(st.players.iter().cloned());
        let mut rows: Vec<PlayerRow> = names
            .into_iter()
            .map(|n| {
                let l = n.to_lowercase();
                PlayerRow {
                    online: st.players.iter().any(|p| p.eq_ignore_ascii_case(&n)),
                    rec: players.get(&n).cloned().unwrap_or_default(),
                    op: ops.contains(&l),
                    whitelisted: wl.contains(&l),
                    banned: bans.contains(&l),
                    name: n,
                }
            })
            .collect();
        rows.sort_by(|a, b| b.online.cmp(&a.online).then(b.rec.last_seen.cmp(&a.rec.last_seen)).then(a.name.cmp(&b.name)));
        Ok(PlayersInfo {
            running: st.status == pcm_core::process::Status::Running,
            rows,
            lists,
            shared_ips: shared,
            chat,
            audit,
        })
    })
    .await
}

/// Действие с игроком: команда серверу. Имя проверяется, чтобы нельзя было подставить лишнее.
#[tauri::command]
pub fn player_action(app: AppHandle, state: State<AppState>, id: String, action: String, name: String, reason: String) -> R<()> {
    if action != "whitelist_on" && action != "whitelist_off" && !pcm_core::players::valid_name(&name) {
        return Err("недопустимое имя игрока".into());
    }
    let reason: String = reason.chars().filter(|c| !c.is_control()).take(120).collect();
    let cmd = match action.as_str() {
        "op" => format!("op {name}"),
        "deop" => format!("deop {name}"),
        "kick" => format!("kick {name} {reason}").trim().to_string(),
        "ban" => format!("ban {name} {reason}").trim().to_string(),
        "pardon" => format!("pardon {name}"),
        "whitelist_add" => format!("whitelist add {name}"),
        "whitelist_remove" => format!("whitelist remove {name}"),
        "whitelist_on" => "whitelist on".into(),
        "whitelist_off" => "whitelist off".into(),
        _ => return Err("неизвестное действие".into()),
    };
    state.pm.system(&id, format!("> {cmd}"));
    crate::social::audit(&app, &id, &cmd, "console");
    state.pm.send(&id, &cmd).map_err(e)
}

/// Проверка уведомлений: тестовое сообщение во все каналы; для Telegram — ещё и имя бота.
#[tauri::command]
pub async fn notify_test(settings: pcm_core::notify::NotifySettings) -> R<String> {
    bg(move || {
        if !settings.any() {
            return Err("не настроен ни Telegram, ни Discord".into());
        }
        let mut ok = Vec::new();
        if settings.telegram_ready() {
            let bot = pcm_core::notify::telegram_me(&settings.telegram_token).map_err(e)?;
            ok.push(format!("Telegram (@{bot})"));
        }
        let errs = pcm_core::notify::send(&settings, "✅ ProtoCore-MCS-Manager: тестовое уведомление. Всё работает.");
        if !errs.is_empty() {
            return Err(errs.join("; "));
        }
        if settings.discord_ready() {
            ok.push("Discord".into());
        }
        Ok(ok.join(", "))
    })
    .await
}
