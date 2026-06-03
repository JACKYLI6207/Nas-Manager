mod commands;
mod config;
mod errors;
mod events;
mod extensions;
mod local_reader;
mod logger;
mod remote_management;
mod share_roots;
mod types;
mod utils;

use anyhow::Context;
use config::Config;
use events::LogEvent;
use parking_lot::RwLock;
use tauri::{Manager, Wry};

use crate::commands::*;

fn generate_context() -> tauri::Context<Wry> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri_specta::Builder::<Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_config,
            save_config,
            get_remote_management_status,
            restart_remote_management,
        ])
        .events(tauri_specta::collect_events![LogEvent]);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let app_data_dir = crate::utils::app_data_dir().context("獲取app_data_dir目錄失敗")?;
            std::fs::create_dir_all(&app_data_dir).context(format!(
                "創建app_data_dir目錄`{}`失敗",
                app_data_dir.display()
            ))?;

            let config = RwLock::new(Config::new(app.handle())?);
            app.manage(config);

            let _ = logger::init(app.handle());

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                remote_management::force_start_async(&handle).await;
            });

            Ok(())
        })
        .run(generate_context())
        .expect("error while running tauri application");
}
