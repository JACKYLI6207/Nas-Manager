use tauri::AppHandle;

use crate::{
    config::Config,
    errors::{CommandError, CommandResult},
    extensions::AppHandleExt,
    local_reader::{self, LocalReaderPages, LocalReaderSource},
    logger,
    volume_binding,
};

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareRootBindResult {
    pub binding: crate::config::ShareRootBinding,
    pub resolved_path: String,
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_config(app: AppHandle) -> Config {
    app.get_config().read().clone()
}

#[tauri::command]
#[specta::specta]
pub fn bind_share_root_path(path: String) -> CommandResult<ShareRootBindResult> {
    let binding = volume_binding::path_to_binding(std::path::Path::new(&path))
        .map_err(|e| CommandError::from("綁定分享路徑失敗", e))?;
    let resolved_path = volume_binding::resolve_binding(&binding)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| path.clone());
    Ok(ShareRootBindResult {
        binding,
        resolved_path,
    })
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn save_config(app: AppHandle, config: Config) -> CommandResult<()> {
    let config_state = app.get_config();
    let enable_file_logger = config.enable_file_logger;
    let file_logger_changed = config_state.read().enable_file_logger != enable_file_logger;

    {
        let mut config = config;
        crate::share_roots::normalize_remote_share_config(&mut config);
        if config.remote_management_enabled {
            crate::remote_management::ensure_token(&mut config);
        }
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("儲存設定失敗", err))?;
    }

    let saved_config = app.get_config().read().clone();
    if saved_config.remote_management_enabled {
        if !crate::remote_management::is_running_with_config(&saved_config) {
            crate::remote_management::sync_async(&app).await;
        } else {
            crate::remote_management::clear_last_error();
        }
    } else {
        crate::remote_management::sync_async(&app).await;
    }

    if file_logger_changed {
        if enable_file_logger {
            logger::reload_file_logger()
                .map_err(|err| CommandError::from("重新加載檔案日誌失敗", err))?;
        } else {
            logger::disable_file_logger()
                .map_err(|err| CommandError::from("禁用檔案日誌失敗", err))?;
        }
    }

    if saved_config.remote_management_enabled {
        let config = app.get_config().read().clone();
        let status = crate::remote_management::get_status(&config);
        if !status.running {
            let detail = status
                .last_error
                .unwrap_or_else(|| "未知原因".to_string());
            return Err(CommandError::from(
                "遠端管理啟動失敗",
                anyhow::anyhow!(detail),
            ));
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_remote_management_status(
    app: AppHandle,
) -> crate::remote_management::RemoteManagementStatus {
    let config = app.get_config().read().clone();
    crate::remote_management::get_status(&config)
}

#[tauri::command]
#[specta::specta]
pub async fn restart_remote_management(
    app: AppHandle,
) -> CommandResult<crate::remote_management::RemoteManagementStatus> {
    {
        let config_state = app.get_config();
        let mut config = config_state.write();
        if !config.remote_management_enabled {
            return Ok(crate::remote_management::get_status(&*config));
        }
        crate::remote_management::ensure_token(&mut config);
        config
            .save(&app)
            .map_err(|err| CommandError::from("儲存設定失敗", err))?;
    }
    crate::remote_management::force_start_async(&app).await;
    let config = app.get_config().read().clone();
    Ok(crate::remote_management::get_status(&config))
}

#[tauri::command]
#[specta::specta]
pub fn list_local_reader_sources(
    app: AppHandle,
    folder_path: String,
) -> CommandResult<Vec<LocalReaderSource>> {
    local_reader::list_local_reader_sources_with_app(&app, &folder_path)
        .map_err(|err| CommandError::from("列出本地漫畫來源失敗", err))
}

#[tauri::command]
#[specta::specta]
pub fn prepare_local_reader_zip(app: AppHandle, source_uri: String) -> CommandResult<String> {
    local_reader::prepare_local_reader_zip(&app, &source_uri)
        .map_err(|err| CommandError::from("準備 ZIP 檔案失敗", err))
}

#[tauri::command]
#[specta::specta]
pub fn load_local_reader_pages(
    app: AppHandle,
    source_path: String,
    source_kind: Option<local_reader::LocalReaderSourceKind>,
) -> CommandResult<LocalReaderPages> {
    local_reader::load_local_reader_pages_with_app(&app, &source_path, source_kind)
        .map_err(|err| CommandError::from("載入本地漫畫頁面失敗", err))
}

#[tauri::command]
#[specta::specta]
pub fn get_local_reader_image(app: AppHandle, page_id: String) -> CommandResult<Vec<u8>> {
    local_reader::read_local_reader_image_with_app(&app, &page_id)
        .map_err(|err| CommandError::from("讀取本地漫畫圖片失敗", err))
}

#[tauri::command]
#[specta::specta]
pub fn close_local_reader_zip_session() -> CommandResult<()> {
    local_reader::close_zip_reader_session();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn enter_remote_wifi_mode(_app: tauri::AppHandle) -> String {
    String::new()
}

#[tauri::command]
#[specta::specta]
pub fn leave_remote_wifi_mode(_app: tauri::AppHandle) {}

#[tauri::command(async)]
#[specta::specta]
pub async fn scan_lan_remote_pcs(
    app: tauri::AppHandle,
) -> CommandResult<crate::pc_remote_discovery::RemotePcScanResult> {
    crate::pc_remote_discovery::scan_lan_remote_pcs(&app)
        .await
        .map_err(|err| CommandError::from("掃描區網 PC 失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn test_remote_pc_connection(
    app: tauri::AppHandle,
    hosts: Vec<String>,
    port: u16,
    skip_wifi_bind: Option<bool>,
) -> crate::pc_remote_discovery::RemotePcConnectionResult {
    crate::pc_remote_discovery::test_remote_pc_connection(
        Some(&app),
        hosts,
        port,
        skip_wifi_bind.unwrap_or(false),
    )
    .await
}

#[tauri::command(async)]
#[specta::specta]
pub async fn list_remote_pc_directory(
    host: String,
    port: u16,
    path: String,
) -> CommandResult<crate::pc_remote_discovery::RemotePcBrowseResult> {
    crate::pc_remote_discovery::list_remote_pc_directory(&host, port, &path)
        .await
        .map_err(|err| CommandError::from("讀取 PC 資料夾失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn fetch_remote_comic_pages(
    host: String,
    port: u16,
    path: String,
) -> CommandResult<crate::remote_comic::RemoteComicPagesResult> {
    crate::remote_comic::fetch_remote_comic_pages(&host, port, &path)
        .await
        .map_err(|err| CommandError::from("讀取漫畫頁面清單失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn fetch_remote_comic_page_image(
    host: String,
    port: u16,
    path: String,
    entry: String,
) -> CommandResult<Vec<u8>> {
    crate::remote_comic::fetch_remote_comic_page_image(&host, port, &path, &entry)
        .await
        .map_err(|err| CommandError::from("讀取漫畫頁面失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn pick_remote_transfer_destination(
    app: AppHandle,
) -> CommandResult<Option<String>> {
    crate::desktop_commands::pick_writable_folder_path(&app).await
}

#[tauri::command(async)]
#[specta::specta]
pub async fn transfer_remote_pc_files(
    app: AppHandle,
    host: String,
    port: u16,
    selections: Vec<crate::remote_pc_transfer::RemotePcTransferSelection>,
    dest_tree_uri: String,
) -> CommandResult<()> {
    crate::remote_pc_transfer::transfer_remote_pc_files(
        &app, &host, port, &selections, &dest_tree_uri,
    )
    .await
    .map_err(|err| CommandError::from("遠端下載失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn plan_remote_pc_upload(
    app: AppHandle,
    host: String,
    port: u16,
    pc_dest_dir: String,
    source_uri: String,
    kind: String,
) -> CommandResult<crate::remote_pc_upload::RemoteUploadPlan> {
    crate::remote_pc_upload::plan_remote_pc_upload(
        &app, &host, port, &pc_dest_dir, &source_uri, &kind,
    )
    .await
    .map_err(|err| CommandError::from("規劃上傳失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub fn cancel_remote_pc_transfer() {
    crate::remote_pc_transfer::request_remote_transfer_cancel();
}

#[tauri::command(async)]
#[specta::specta]
pub async fn upload_remote_pc_files(
    app: AppHandle,
    host: String,
    port: u16,
    files: Vec<crate::remote_pc_upload::RemoteUploadPlanItem>,
    on_conflict: String,
) -> CommandResult<()> {
    crate::remote_pc_upload::upload_remote_pc_files(&app, &host, port, files, &on_conflict)
        .await
        .map_err(|err| CommandError::from("遠端上傳失敗", err))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn remote_pc_file_op(
    host: String,
    port: u16,
    action: String,
    paths: Vec<String>,
    dest_path: String,
    new_name: String,
) -> CommandResult<crate::remote_pc_file_op::RemotePcFileOpResult> {
    crate::remote_pc_file_op::remote_pc_file_op(
        &host,
        port,
        &action,
        &paths,
        &dest_path,
        &new_name,
    )
    .await
    .map_err(|err| CommandError::from("PC 檔案操作失敗", err))
}
