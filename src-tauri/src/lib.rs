mod commands;
mod db;
mod documents;
mod error;
mod llm;
mod mail;
mod models;
mod reports;
mod scheduler;
mod secrets;
mod templates;

use db::Database;
use reports::ActiveGenerations;
use secrets::SecretStore;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tokio::sync::{Mutex, Notify};

pub struct AppState {
    pub db: Database,
    pub secrets: SecretStore,
    pub active_generations: ActiveGenerations,
    pub scheduler_notify: Arc<Notify>,
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let db = tauri::async_runtime::block_on(Database::open(app_data_dir))?;
            let secrets = SecretStore::system();
            tauri::async_runtime::block_on(secrets.migrate_plaintext(&db.pool))?;
            let active_generations = Arc::new(Mutex::new(HashSet::new()));
            let scheduler_notify = Arc::new(Notify::new());
            scheduler::start(
                db.pool.clone(),
                secrets.clone(),
                active_generations.clone(),
                scheduler_notify.clone(),
            );
            app.manage(AppState {
                db,
                secrets,
                active_generations,
                scheduler_notify,
            });

            let show = MenuItem::with_id(app, "show", "打开 Worklog", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "彻底退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Worklog")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_work_logs,
            commands::create_work_log,
            commands::update_work_log,
            commands::delete_work_log,
            commands::list_templates,
            commands::create_template,
            commands::update_template,
            commands::delete_template,
            commands::import_template_example,
            commands::optimize_template,
            commands::list_reports,
            commands::update_report,
            commands::delete_report,
            commands::optimize_report,
            commands::generate_report,
            commands::export_report_docx,
            commands::list_report_email_deliveries,
            commands::send_report_email,
            commands::get_llm_setting,
            commands::list_llm_settings,
            commands::create_llm_setting,
            commands::update_llm_setting,
            commands::apply_llm_setting,
            commands::delete_llm_setting,
            commands::get_email_setting,
            commands::update_email_setting,
            commands::send_email_test,
            commands::list_recipients,
            commands::create_recipient,
            commands::update_recipient,
            commands::delete_recipient,
            commands::list_report_schedules,
            commands::update_report_schedule,
            commands::get_desktop_preferences,
            commands::set_launch_at_login,
            commands::get_startup_migration,
            commands::import_legacy_database,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Worklog");
}
