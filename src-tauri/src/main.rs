//! Точка входа: окно, трей, регистрация команд.
//!
//! Экономия ресурсов: при закрытии окна (если включено «сворачивать в трей»)
//! окно **уничтожается** целиком — веб-движок выгружается из памяти.
//! В фоне остаются только ядро на Rust и запущенные серверы.
//! Клик по иконке в трее создаёт окно заново, консоль подтягивается из буфера.

// без чёрного окна консоли в релизной сборке Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bisect_run;
mod commands;
mod netmon;
mod scheduler;
mod social;
mod state;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pcm_core::config::{AppPaths, Config};
use pcm_core::process::ProcessManager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use state::{AppState, TauriSink};

const MAIN: &str = "main";

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("ProtoCore-MCS-Manager")
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 560.0)
        .decorations(false)
        .transparent(false)
        .build();
}

/// Выход: мягко останавливаем все серверы, затем закрываемся.
pub(crate) fn quit(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        *state.quitting.lock().unwrap() = true;
        let ids = state.pm.running_ids();
        for id in &ids {
            let t = state
                .config
                .lock()
                .unwrap()
                .server(id)
                .map(|s| s.launch.stop_timeout_sec)
                .unwrap_or(60);
            let _ = state.pm.stop_and_wait(id, Duration::from_secs(t.max(10) as u64));
        }
        app.exit(0);
    });
}

fn main() {
    // режим замены при обновлении: этот процесс — новая версия, встаём на место старой
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(pcm_core::update::FINISH_ARG) {
        if let Some(target) = args.get(2) {
            let _ = pcm_core::update::finish(std::path::Path::new(target));
        }
        return;
    }
    pcm_core::update::cleanup();
    let paths = AppPaths::from_exe();
    let _ = paths.ensure();
    pcm_core::java::set_managed_root(paths.base.join("java"));
    let config = Config::load(&paths);
    let sink = Arc::new(TauriSink::default());
    let pm = Arc::new(ProcessManager::new(sink.clone(), config.console_lines));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(scheduler::Sched::default())
        .manage(netmon::NetMon::default())
        .manage(social::Social::default())
        .manage(bisect_run::BisectState::default())
        .manage(AppState {
            paths,
            config: Mutex::new(config),
            pm,
            busy: Mutex::new(HashSet::new()),
            quitting: Mutex::new(false),
            crash_log: Mutex::new(Vec::new()),
            quarantine_rounds: Mutex::new(Default::default()),
        })
        .setup(move |app| {
            let _ = sink.app.set(app.handle().clone());
            scheduler::start(app.handle().clone());
            netmon::start(app.handle().clone());
            social::start_bot(app.handle().clone());

            let open = MenuItem::with_id(app, "open", "Открыть / Open", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "quit", "Выход / Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &exit])?;
            let mut tray = TrayIconBuilder::with_id("pcm")
                .tooltip("ProtoCore-MCS-Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_window(app),
                    "quit" => quit(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            show_window(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let to_tray = state.config.lock().unwrap().close_to_tray;
                if !to_tray && !*state.quitting.lock().unwrap() {
                    // без трея закрытие окна = выход (серверы останавливаются)
                    quit(app);
                }
                // иначе окно просто уничтожается, приложение живёт в трее
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_settings,
            commands::get_branding,
            commands::detect_folder,
            commands::add_server,
            commands::remove_server,
            commands::select_server,
            commands::reorder_servers,
            commands::update_server,
            commands::redetect_server,
            commands::launch_preview,
            commands::accept_eula,
            commands::read_properties,
            commands::write_properties,
            commands::start_server,
            commands::stop_server,
            commands::kill_server,
            commands::restart_server,
            commands::send_command,
            commands::get_log,
            commands::clear_log,
            commands::get_metrics,
            commands::list_mods,
            commands::set_mod_enabled,
            commands::set_mods_enabled,
            commands::disable_client_mods,
            commands::list_plugins,
            commands::set_plugin_enabled,
            commands::download_restic,
            commands::restic_status,
            commands::list_snapshots,
            commands::backup_now,
            commands::delete_snapshot,
            commands::rollback,
            commands::check_repo,
            commands::check_update,
            commands::core_versions,
            commands::core_builds,
            commands::create_server,
            commands::update_core,
            commands::java_check,
            commands::catalog_search,
            commands::catalog_versions,
            commands::catalog_plan,
            commands::catalog_apply,
            commands::catalog_updates,
            commands::catalog_fix,
            commands::check_cf_key,
            commands::open_path,
            commands::get_monitor,
            commands::bisect_start,
            commands::bisect_cancel,
            commands::bisect_info,
            commands::get_advice,
            commands::net_report,
            commands::modpack_inspect,
            commands::players_info,
            commands::snapshot_ls,
            commands::snapshot_restore,
            commands::snapshot_diff,
            commands::copy_now,
            commands::verify_repo,
            commands::get_changes,
            commands::java_list,
            commands::java_download,
            commands::java_required,
            commands::preflight,
            commands::logs_info,
            commands::logs_search,
            commands::logs_clean,
            commands::sandbox_update,
            commands::set_simple_mode,
            commands::player_action,
            commands::notify_test,
            commands::modpack_import,
            commands::modpack_export,
            commands::connect_info,
            commands::upnp_open,
            commands::upnp_close,
            commands::analyze_logs,
            commands::save_compressed,
            commands::quarantine_mod,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("не удалось запустить приложение");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            let state = app.state::<AppState>();
            // окно закрыли, но мы в трее — не выходим (code == None значит «закрылись все окна»)
            if code.is_none() && !*state.quitting.lock().unwrap() {
                api.prevent_exit();
            }
        }
    });
}
