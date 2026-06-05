use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

use crate::errors::{CommandError, CommandResult};
use crate::mobile_settings::MobileSettings;

#[cfg(not(target_os = "android"))]
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct PlayVideoResult {
    pub cancelled: bool,
    pub background: bool,
}

#[cfg(target_os = "android")]
use crate::clipboard_plugin::clipboard_plugin;
#[cfg(target_os = "android")]
use crate::folder_picker::folder_picker;
#[cfg(target_os = "android")]
use crate::local_video_player::{build_pc_stream_url, local_video_player, PlayVideoResult};
#[cfg(target_os = "android")]
use crate::pc_remote_discovery::ensure_pc_remote_api_v5;

#[cfg(not(target_os = "android"))]
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

#[cfg(target_os = "android")]
async fn pick_folder_path(app: &AppHandle) -> CommandResult<Option<String>> {
    let picker = folder_picker(app)?;
    Ok(picker.pick_document_tree()?)
}

#[tauri::command]
#[specta::specta]
pub fn copy_text_to_clipboard(app: AppHandle, text: String) -> CommandResult<()> {
    #[cfg(target_os = "android")]
    {
        clipboard_plugin(&app)?.copy_text(&text)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, text);
        Ok(())
    }
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

pub(crate) async fn pick_writable_folder_path(app: &AppHandle) -> CommandResult<Option<String>> {
    let Some(path) = pick_folder_path(app).await? else {
        return Ok(None);
    };
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(app)?;
        let writable = picker.probe_tree_writable(&path)?;
        if !writable {
            return Err(CommandError::from(
                "目錄不可寫",
                anyhow::anyhow!("請改選可寫入目錄（目前目錄僅可讀取）"),
            ));
        }
    }
    Ok(Some(path))
}

#[tauri::command]
#[specta::specta]
pub fn pick_local_reader_zip(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(&app)?;
        return picker.pick_open_zip();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn pick_local_reader_folder(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(&app)?;
        return picker.pick_document_tree();
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(pick_folder_path(&app).await?)
    }
}

#[tauri::command]
#[specta::specta]
pub fn pick_local_video_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        return player.pick_local_video();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub fn play_local_video_file(
    app: AppHandle,
    uri: String,
    title: Option<String>,
    subtitle_uris: Option<Vec<String>>,
    resume_only: Option<bool>,
) -> CommandResult<PlayVideoResult> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        let subs = subtitle_uris.unwrap_or_default();
        return player.play_local_video(
            &uri,
            title.as_deref(),
            &subs,
            None,
            None,
            None,
            Some(0),
            Some(resume_only.unwrap_or(false)),
        );
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, uri, title, subtitle_uris, resume_only);
        Err(CommandError::from(
            "僅支援 Android",
            anyhow::anyhow!("桌面版無內建影片播放器"),
        ))
    }
}

#[tauri::command]
#[specta::specta]
pub fn stop_video_playback(app: AppHandle) -> CommandResult<()> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        return player.stop_video_playback();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct VideoPlaybackProgress {
    pub position_ms: i64,
    pub duration_ms: i64,
}

#[tauri::command]
#[specta::specta]
pub fn get_video_playback_progress(
    app: AppHandle,
    host: String,
    port: u16,
    rel_path: String,
) -> CommandResult<VideoPlaybackProgress> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        let p = player.get_video_playback_progress(&host, port, &rel_path)?;
        return Ok(VideoPlaybackProgress {
            position_ms: p.position_ms,
            duration_ms: p.duration_ms,
        });
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, host, port, rel_path);
        Ok(VideoPlaybackProgress {
            position_ms: 0,
            duration_ms: 0,
        })
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_background_playback_session(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        return player.get_background_playback_session();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
#[specta::specta]
pub fn pick_local_subtitle_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        return player.pick_local_subtitle();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
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
    app: AppHandle,
    jobs: Vec<StreamPlaylistJob>,
    current_rel_path: Option<String>,
) -> CommandResult<()> {
    #[cfg(target_os = "android")]
    {
        let player = local_video_player(&app)?;
        return player.sync_stream_playlist(&jobs, current_rel_path.as_deref());
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, jobs, current_rel_path);
        Ok(())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn play_remote_pc_video(
    app: AppHandle,
    host: String,
    port: u16,
    rel_path: String,
    title: Option<String>,
    subtitle_uris: Option<Vec<String>>,
    start_position_ms: Option<i64>,
    resume_only: Option<bool>,
) -> CommandResult<PlayVideoResult> {
    #[cfg(target_os = "android")]
    {
        ensure_pc_remote_api_v5(&host, port)
            .await
            .map_err(|e| CommandError::from("PC 不支援影片串流", e))?;
        let stream_url = build_pc_stream_url(&host, port, &rel_path);
        let player = local_video_player(&app)?;
        let subs = subtitle_uris.unwrap_or_default();
        return player.play_local_video(
            &stream_url,
            title.as_deref(),
            &subs,
            Some(host.as_str()),
            Some(port),
            Some(rel_path.as_str()),
            Some(start_position_ms.unwrap_or(0).max(0)),
            Some(resume_only.unwrap_or(false)),
        );
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (
            app,
            host,
            port,
            rel_path,
            title,
            subtitle_uris,
            start_position_ms,
            resume_only,
        );
        Err(CommandError::from(
            "僅支援 Android",
            anyhow::anyhow!("桌面版無內建影片播放器"),
        ))
    }
}
