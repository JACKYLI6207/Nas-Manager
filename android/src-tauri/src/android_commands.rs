use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

use crate::errors::{CommandError, CommandResult};
use crate::mobile_settings::MobileSettings;

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
        .map_err(|_| CommandError::from("?豢?鞈?憭曉仃??, anyhow::anyhow!("撠店獢歇??")))?;
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
    MobileSettings::load(&app).map_err(|e| CommandError::from("霈?身摰仃??, e))
}

#[tauri::command]
#[specta::specta]
pub fn save_mobile_settings(app: AppHandle, settings: MobileSettings) -> CommandResult<()> {
    settings
        .save(&app)
        .map_err(|e| CommandError::from("?脣?閮剖?憭望?", e))
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
                "?桅?銝撖?,
                anyhow::anyhow!("隢?詨撖怠?桅?嚗????航???"),
            ));
        }
    }
    Ok(Some(path))
}

/// `persist = false` ???頝臬?嚗??嗉?摮?撠蝑?甈⊥批神?伐?嚗?閬神敹怎鞈?憭曇身摰?#[tauri::command]
#[specta::specta]
pub async fn pick_category_directory(
    app: AppHandle,
    persist: Option<bool>,
) -> CommandResult<Option<String>> {
    let Some(path) = pick_writable_folder_path(&app).await? else {
        return Ok(None);
    };
    if persist.unwrap_or(true) {
        let mut settings =
            MobileSettings::load(&app).map_err(|e| CommandError::from("霈?身摰仃??, e))?;
        settings.category_directory = Some(path.clone());
        settings
            .save(&app)
            .map_err(|e| CommandError::from("?脣?閮剖?憭望?", e))?;
    }
    Ok(Some(path))
}

#[tauri::command]
#[specta::specta]
pub async fn pick_import_archive_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(&app)?;
        return Ok(picker.pick_open_archive()?);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(CommandError::from(
            "銝?渡?撟喳",
            anyhow::anyhow!("??Android ?舫??瑼?獢?),
        ))
    }
}

#[tauri::command]
#[specta::specta]
pub fn read_import_archive_file(app: AppHandle, path: String) -> CommandResult<String> {
    #[cfg(target_os = "android")]
    if path.starts_with("content://") {
        let picker = folder_picker(&app)?;
        return picker.read_text(&path);
    }
    std::fs::read_to_string(&path).map_err(|err| {
        CommandError::from(
            "霈??瑼仃??,
            anyhow::anyhow!("霈??瑼{path}`憭望?: {err}"),
        )
    })
}

#[tauri::command]
#[specta::specta]
pub async fn pick_korean_txt_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(&app)?;
        let Some(uri) = picker.pick_open_txt()? else {
            return Ok(None);
        };
        {
            let config_state = app.state::<parking_lot::RwLock<crate::config::Config>>();
            let mut config = config_state.write();
            config.korean_txt_catalog_dir = std::path::PathBuf::from(&uri);
            let _ = config.save(&app);
        }
        return Ok(Some(uri));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(CommandError::from(
            "銝?渡?撟喳",
            anyhow::anyhow!("??Android ?舫??TXT 瑼?"),
        ))
    }
}

#[tauri::command]
#[specta::specta]
pub async fn pick_download_directory(app: AppHandle) -> CommandResult<Option<String>> {
    let Some(path) = pick_folder_path(&app).await? else {
        return Ok(None);
    };
    #[cfg(target_os = "android")]
    {
        let picker = folder_picker(&app)?;
        let writable = picker.probe_tree_writable(&path)?;
        if !writable {
            return Err(CommandError::from(
                "銝??桅?銝撖?,
                anyhow::anyhow!("隢?詨撖怠?桅?嚗????航???"),
            ));
        }
    }
    let mut settings = MobileSettings::load(&app).map_err(|e| CommandError::from("霈?身摰仃??, e))?;
    settings.download_directory = Some(path.clone());
    {
        let config_state = app.state::<parking_lot::RwLock<crate::config::Config>>();
        let mut config = config_state.write();
        config.download_dir = std::path::PathBuf::from(&path);
        let _ = config.save(&app);
    }
    settings
        .save(&app)
        .map_err(|e| CommandError::from("?脣?閮剖?憭望?", e))?;
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
            "???Android",
            anyhow::anyhow!("獢??批遣敶梁??剜??),
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
            .map_err(|e| CommandError::from("PC 銝?游蔣?葡瘚?, e))?;
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
            "???Android",
            anyhow::anyhow!("獢??批遣敶梁??剜??),
        ))
    }
}

