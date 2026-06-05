mod android_commands;
mod commands;
mod config;
mod errors;
mod events;
mod extensions;
#[cfg(target_os = "android")]
mod clipboard_plugin;
#[cfg(target_os = "android")]
mod folder_picker;
#[cfg(target_os = "android")]
mod lan_discovery;
#[cfg(target_os = "android")]
mod local_video_player;
mod local_reader;
mod logger;
mod mobile_settings;
mod pc_remote_discovery;
mod remote_comic;
mod remote_pc_file_op;
mod remote_pc_transfer;
mod remote_pc_upload;
mod types;
mod utils;

use anyhow::Context;
use config::Config;
use events::LogEvent;
use mobile_settings::MobileSettings;
use parking_lot::RwLock;
use remote_pc_transfer::RemoteTransferProgressEvent;
use tauri::{Manager, Wry};

use crate::{android_commands::*, commands::*};

fn generate_context() -> tauri::Context<Wry> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri_specta::Builder::<Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_mobile_settings,
            save_mobile_settings,
            pick_local_reader_zip,
            pick_local_reader_folder,
            pick_local_video_file,
            pick_local_subtitle_file,
            play_local_video_file,
            play_remote_pc_video,
            get_background_playback_session,
            get_video_playback_progress,
            stop_video_playback,
            sync_stream_playlist,
            copy_text_to_clipboard,
            get_config,
            save_config,
            list_local_reader_sources,
            prepare_local_reader_zip,
            load_local_reader_pages,
            get_local_reader_image,
            close_local_reader_zip_session,
            scan_lan_remote_pcs,
            enter_remote_wifi_mode,
            leave_remote_wifi_mode,
            test_remote_pc_connection,
            list_remote_pc_directory,
            fetch_remote_comic_pages,
            fetch_remote_comic_page_image,
            pick_remote_transfer_destination,
            transfer_remote_pc_files,
            cancel_remote_pc_transfer,
            pick_remote_upload_file,
            pick_remote_upload_folder,
            plan_remote_pc_upload,
            upload_remote_pc_files,
            remote_pc_file_op,
        ])
        .events(tauri_specta::collect_events![LogEvent, RemoteTransferProgressEvent]);

    let mut tauri_builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    {
        tauri_builder = tauri_builder
            .plugin(crate::clipboard_plugin::init())
            .plugin(crate::folder_picker::init())
            .plugin(crate::lan_discovery::init())
            .plugin(crate::local_video_player::init());
    }
    tauri_builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let app_data_dir =
                crate::utils::app_data_dir(app.handle()).context("獲取 app_data_dir 失敗")?;
            std::fs::create_dir_all(&app_data_dir).context("創建 app_data_dir 失敗")?;

            let mut config = Config::new(app.handle())?;
            if let Ok(mobile) = MobileSettings::load(app.handle()) {
                if let Some(dir) = mobile.download_directory.as_ref() {
                    config.download_dir = std::path::PathBuf::from(dir);
                }
            }
            app.manage(RwLock::new(config));

            let _ = logger::init(app.handle());

            Ok(())
        })
        .run(generate_context())
        .expect("error while running tauri application");
}
