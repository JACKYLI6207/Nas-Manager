use tauri::AppHandle;

use crate::{
    config::Config,
    errors::{CommandError, CommandResult},
    extensions::AppHandleExt,
    local_reader::{self, LocalReaderPages, LocalReaderSource},
    logger,
};

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_config(app: AppHandle) -> Config {
    app.get_config().read().clone()
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn save_config(app: AppHandle, config: Config) -> CommandResult<()> {
    let config_state = app.get_config();
    let file_logger_changed = config_state.read().enable_file_logger != config.enable_file_logger;
    let enable_file_logger = config.enable_file_logger;
    {
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("儲存設定失敗", err))?;
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
    Ok(())
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
pub fn enter_remote_wifi_mode(app: tauri::AppHandle) -> String {
    #[cfg(target_os = "android")]
    {
        if let Some(state) = app.try_state::<crate::lan_discovery::LanDiscovery<tauri::Wry>>() {
            return state.begin_wifi_session();
        }
        return "LanDiscoveryPlugin 未載入".to_string();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        String::new()
    }
}

#[tauri::command]
#[specta::specta]
pub fn leave_remote_wifi_mode(app: tauri::AppHandle) {
    #[cfg(target_os = "android")]
    if let Some(state) = app.try_state::<crate::lan_discovery::LanDiscovery<tauri::Wry>>() {
        state.end_wifi_session();
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

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
    crate::android_commands::pick_writable_folder_path(&app).await
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
pub async fn pick_remote_upload_file(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = crate::folder_picker::folder_picker(&app)
            .map_err(|e| CommandError::from("選擇上傳檔案失敗", anyhow::anyhow!("{}", e.err_message)))?;
        return picker
            .pick_upload_document()
            .map_err(|e| CommandError::from("選擇上傳檔案失敗", anyhow::anyhow!("{}", e.err_message)));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command(async)]
#[specta::specta]
pub async fn pick_remote_upload_folder(app: AppHandle) -> CommandResult<Option<String>> {
    #[cfg(target_os = "android")]
    {
        let picker = crate::folder_picker::folder_picker(&app)
            .map_err(|e| CommandError::from("選擇上傳資料夾失敗", anyhow::anyhow!("{}", e.err_message)))?;
        return picker
            .pick_upload_folder()
            .map_err(|e| CommandError::from("選擇上傳資料夾失敗", anyhow::anyhow!("{}", e.err_message)));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
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
