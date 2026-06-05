use std::path::Path;
use std::process::Command;

use base64::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::errors::{CommandError, CommandResult};
use crate::mobile_settings::MobileSettings;
use crate::pc_remote_discovery::ensure_pc_remote_api_v5;
use crate::video_stream_prep::{emit_video_stream_prep, prefetch_stream_for_playback, VideoStreamPrepProgressEvent};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PlayVideoResult {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub background: bool,
}

pub fn build_pc_stream_url(host: &str, port: u16, rel_path: &str) -> String {
    let path_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rel_path.as_bytes());
    format!("http://{host}:{port}/api/v1/stream?path_b64={path_b64}")
}

fn external_player_exe(app: &AppHandle) -> Option<String> {
    MobileSettings::load(app)
        .ok()
        .and_then(|s| s.external_video_player_path)
        .filter(|p| !p.trim().is_empty())
}

fn launch_external_player(app: &AppHandle, target: &str) -> CommandResult<()> {
    if let Some(exe) = external_player_exe(app) {
        Command::new(&exe)
            .arg(target)
            .spawn()
            .map_err(|e| CommandError::from("啟動外部播放器失敗", e))?;
        return Ok(());
    }
    app.opener()
        .open_path(target, None::<&str>)
        .map_err(|e| CommandError::from("以系統預設程式開啟失敗", e))
}

#[tauri::command]
#[specta::specta]
pub fn copy_text_to_clipboard(_app: AppHandle, text: String) -> CommandResult<()> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| CommandError::from("複製到剪貼簿失敗", e))
}

#[tauri::command]
#[specta::specta]
pub fn get_mobile_settings(app: AppHandle) -> CommandResult<MobileSettings> {
    MobileSettings::load(&app).map_err(|e| CommandError::from("讀取設定失敗", e))
}

#[tauri::command]
#[specta::specta]
pub fn save_mobile_settings(app: AppHandle, settings: MobileSettings) -> CommandResult<()> {
    settings
        .save(&app)
        .map_err(|e| CommandError::from("儲存設定失敗", e))
}

async fn pick_file_path(app: &AppHandle, filters: Option<Vec<(&str, &[&str])>>) -> CommandResult<Option<String>> {
    let (tx, rx) = oneshot::channel();
    let mut builder = app.dialog().file();
    if let Some(list) = filters {
        for (name, exts) in list {
            builder = builder.add_filter(name, exts);
        }
    }
    builder.pick_file(move |path| {
        let _ = tx.send(path);
    });
    let picked = rx
        .await
        .map_err(|_| CommandError::from("選擇檔案失敗", anyhow::anyhow!("對話框已關閉")))?;
    Ok(picked.map(|p| p.to_string()))
}

async fn pick_folder_path(app: &AppHandle) -> CommandResult<Option<String>> {
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::from("選擇資料夾失敗", anyhow::anyhow!("對話框已關閉")))?;
    Ok(picked.map(|p| p.to_string()))
}

pub(crate) async fn pick_writable_folder_path(app: &AppHandle) -> CommandResult<Option<String>> {
    pick_folder_path(app).await
}

#[tauri::command]
#[specta::specta]
pub async fn pick_local_reader_zip(app: AppHandle) -> CommandResult<Option<String>> {
    pick_file_path(
        &app,
        Some(vec![("ZIP 漫畫", &["zip", "cbz"])]),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn pick_local_reader_folder(app: AppHandle) -> CommandResult<Option<String>> {
    pick_folder_path(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn pick_local_video_file(app: AppHandle) -> CommandResult<Option<String>> {
    pick_file_path(
        &app,
        Some(vec![("影片", &["mp4", "mkv", "avi", "webm", "mov", "wmv", "flv", "rmvb", "m4v"])]),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn pick_local_subtitle_file(app: AppHandle) -> CommandResult<Option<String>> {
    pick_file_path(
        &app,
        Some(vec![("字幕", &["srt", "ass", "vtt", "ssa"])]),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub fn play_local_video_file(
    app: AppHandle,
    uri: String,
    _title: Option<String>,
    _subtitle_uris: Option<Vec<String>>,
    _resume_only: Option<bool>,
) -> CommandResult<PlayVideoResult> {
    let path = Path::new(&uri);
    if !path.is_file() {
        return Ok(PlayVideoResult {
            error: Some("找不到影片檔案".to_string()),
            background: false,
        });
    }
    if let Err(e) = launch_external_player(&app, &uri) {
        return Ok(PlayVideoResult {
            error: Some(e.err_message),
            background: false,
        });
    }
    Ok(PlayVideoResult {
        error: None,
        background: false,
    })
}

#[tauri::command]
#[specta::specta]
pub fn stop_video_playback(_app: AppHandle) -> CommandResult<()> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_background_playback_session(_app: AppHandle) -> CommandResult<Option<String>> {
    Ok(None)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct StreamPlaylistJob {
    pub host: String,
    pub port: u16,
    #[serde(rename = "relPath")]
    pub rel_path: String,
    pub title: String,
}

#[tauri::command]
#[specta::specta]
pub fn sync_stream_playlist(
    _app: AppHandle,
    _jobs: Vec<StreamPlaylistJob>,
    _current_rel_path: Option<String>,
) -> CommandResult<()> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn play_remote_pc_video(
    app: AppHandle,
    host: String,
    port: u16,
    rel_path: String,
    _title: Option<String>,
    _subtitle_uris: Option<Vec<String>>,
    _start_position_ms: Option<i64>,
    _resume_only: Option<bool>,
) -> CommandResult<PlayVideoResult> {
    emit_video_stream_prep(
        &app,
        VideoStreamPrepProgressEvent {
            phase: "checking".into(),
            message: "正在連線至 PC…".into(),
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            finished: false,
            error: None,
        },
    );

    if let Err(e) = ensure_pc_remote_api_v5(&host, port).await {
        let msg = format!("PC 不支援影片串流：{e}");
        emit_video_stream_prep(
            &app,
            VideoStreamPrepProgressEvent {
                phase: "error".into(),
                message: msg.clone(),
                bytes_done: 0,
                bytes_total: 0,
                speed_bps: 0,
                finished: true,
                error: Some(msg.clone()),
            },
        );
        return Err(CommandError::from("PC 不支援影片串流", e));
    }

    let stream_url = build_pc_stream_url(&host, port, &rel_path);

    if let Err(e) = prefetch_stream_for_playback(&app, &stream_url).await {
        let msg = e.to_string();
        emit_video_stream_prep(
            &app,
            VideoStreamPrepProgressEvent {
                phase: "error".into(),
                message: msg.clone(),
                bytes_done: 0,
                bytes_total: 0,
                speed_bps: 0,
                finished: true,
                error: Some(msg.clone()),
            },
        );
        return Ok(PlayVideoResult {
            error: Some(msg),
            background: false,
        });
    }

    emit_video_stream_prep(
        &app,
        VideoStreamPrepProgressEvent {
            phase: "launching".into(),
            message: "正在啟動外部播放器…".into(),
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            finished: false,
            error: None,
        },
    );

    if let Err(e) = launch_external_player(&app, &stream_url) {
        emit_video_stream_prep(
            &app,
            VideoStreamPrepProgressEvent {
                phase: "error".into(),
                message: e.err_message.clone(),
                bytes_done: 0,
                bytes_total: 0,
                speed_bps: 0,
                finished: true,
                error: Some(e.err_message.clone()),
            },
        );
        return Ok(PlayVideoResult {
            error: Some(e.err_message),
            background: false,
        });
    }

    emit_video_stream_prep(
        &app,
        VideoStreamPrepProgressEvent {
            phase: "done".into(),
            message: "已啟動播放器".into(),
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            finished: true,
            error: None,
        },
    );

    Ok(PlayVideoResult {
        error: None,
        background: false,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn pick_remote_upload_file(app: AppHandle) -> CommandResult<Option<String>> {
    pick_file_path(&app, None).await
}

#[tauri::command]
#[specta::specta]
pub async fn pick_remote_upload_folder(app: AppHandle) -> CommandResult<Option<String>> {
    pick_folder_path(&app).await
}

#[tauri::command]
#[specta::specta]
pub fn list_video_player_options() -> Vec<crate::video_player_discovery::VideoPlayerOption> {
    crate::video_player_discovery::list_video_player_options()
}

#[tauri::command]
#[specta::specta]
pub async fn pick_external_video_player_exe(app: AppHandle) -> CommandResult<Option<String>> {
    pick_file_path(&app, Some(vec![("程式", &["exe"])])).await
}

#[tauri::command]
#[specta::specta]
pub fn set_default_video_player(
    app: AppHandle,
    exe_path: Option<String>,
) -> CommandResult<crate::video_player_discovery::VideoPlayerOption> {
    let mut settings = MobileSettings::load(&app)
        .map_err(|e| CommandError::from("讀取設定失敗", e))?;
    settings.external_video_player_path = exe_path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    settings
        .save(&app)
        .map_err(|e| CommandError::from("儲存設定失敗", e))?;

    if let Some(path) = settings.external_video_player_path.as_deref() {
        if let Some(found) = crate::video_player_discovery::find_player_by_exe(path) {
            return Ok(found);
        }
        return Ok(crate::video_player_discovery::VideoPlayerOption {
            id: crate::video_player_discovery::player_id_from_path(path),
            name: Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string()),
            exe_path: Some(path.to_string()),
            icon_data_url: crate::exe_icon::icon_data_url_for_path(path),
        });
    }

    Ok(crate::video_player_discovery::VideoPlayerOption {
        id: crate::video_player_discovery::SYSTEM_DEFAULT_PLAYER_ID.to_string(),
        name: "系統預設".to_string(),
        exe_path: None,
        icon_data_url: crate::exe_icon::icon_data_url_for_system_default(),
    })
}
