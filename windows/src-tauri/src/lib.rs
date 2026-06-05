mod commands;
mod config;
mod desktop_commands;
mod errors;
mod events;
mod exe_icon;
mod extensions;
mod local_reader;
mod logger;
mod mobile_settings;
mod pc_remote_discovery;
mod remote_comic;
mod remote_management;
mod remote_pc_file_op;
mod remote_pc_transfer;
mod remote_pc_upload;
mod share_roots;
mod types;
mod utils;
mod video_player_discovery;
mod video_stream_prep;
mod volume_binding;

use anyhow::Context;
use config::Config;
use events::LogEvent;
use parking_lot::RwLock;
use remote_pc_transfer::RemoteTransferProgressEvent;
use video_stream_prep::VideoStreamPrepProgressEvent;
use tauri::{Manager, Wry};

use crate::commands::*;
use crate::desktop_commands::*;

fn generate_context() -> tauri::Context<Wry> {
    tauri::generate_context!()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri_specta::Builder::<Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_config,
            save_config,
            bind_share_root_path,
            get_remote_management_status,
            restart_remote_management,
            get_mobile_settings,
            save_mobile_settings,
            copy_text_to_clipboard,
            pick_local_reader_zip,
            pick_local_reader_folder,
            pick_local_video_file,
            pick_local_subtitle_file,
            play_local_video_file,
            play_remote_pc_video,
            get_background_playback_session,
            stop_video_playback,
            sync_stream_playlist,
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
            list_video_player_options,
            pick_external_video_player_exe,
            set_default_video_player,
        ])
        .events(tauri_specta::collect_events![
            LogEvent,
            RemoteTransferProgressEvent,
            VideoStreamPrepProgressEvent,
        ]);

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
