use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::io::AsyncWriteExt;

#[cfg(target_os = "android")]
use crate::folder_picker::folder_picker;
use crate::extensions::AnyhowErrorToStringChain;
use crate::pc_remote_discovery::{ensure_pc_remote_api_v2, list_remote_pc_files};

static REMOTE_TRANSFER_CANCEL: AtomicBool = AtomicBool::new(false);

pub fn reset_remote_transfer_cancel() {
    REMOTE_TRANSFER_CANCEL.store(false, Ordering::Relaxed);
}

pub fn request_remote_transfer_cancel() {
    REMOTE_TRANSFER_CANCEL.store(true, Ordering::Relaxed);
}

pub fn remote_transfer_cancelled() -> bool {
    REMOTE_TRANSFER_CANCEL.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcTransferSelection {
    /// PC 分享根目錄下的完整相對路徑（勾選的檔案或資料夾）
    pub path: String,
    /// 勾選當下正在瀏覽的目錄（用於手機端路徑）
    pub anchor_path: String,
}

#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTransferProgressEvent {
    pub phase: String,
    pub file_index: u32,
    pub file_count: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed_bps: u64,
    pub message: String,
    pub finished: bool,
    pub error: Option<String>,
    /// 除錯用詳細日誌（傳輸失敗時供 UI 顯示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_log: Option<String>,
    /// 成功傳輸的 PC 相對路徑（任務結束時）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub succeeded_paths: Option<Vec<String>>,
    /// 失敗的 PC 相對路徑與錯誤訊息（任務結束時）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_items: Option<Vec<RemoteTransferFailedItem>>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTransferFailedItem {
    pub path: String,
    pub error: String,
}

fn emit_progress(
    app: &AppHandle,
    phase: &str,
    file_index: u32,
    file_count: u32,
    bytes_done: u64,
    bytes_total: u64,
    speed_bps: u64,
    message: &str,
    finished: bool,
    error: Option<String>,
    detail_log: Option<String>,
    succeeded_paths: Option<Vec<String>>,
    failed_items: Option<Vec<RemoteTransferFailedItem>>,
) {
    let _ = RemoteTransferProgressEvent {
        phase: phase.to_string(),
        file_index,
        file_count,
        bytes_done,
        bytes_total,
        speed_bps,
        message: message.to_string(),
        finished,
        error,
        detail_log,
        succeeded_paths,
        failed_items,
    }
    .emit(app);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_progress_active(
    app: &AppHandle,
    phase: &str,
    file_index: u32,
    file_count: u32,
    bytes_done: u64,
    bytes_total: u64,
    speed_bps: u64,
    message: &str,
    finished: bool,
    error: Option<String>,
    detail_log: Option<String>,
) {
    emit_progress(
        app,
        phase,
        file_index,
        file_count,
        bytes_done,
        bytes_total,
        speed_bps,
        message,
        finished,
        error,
        detail_log,
        None,
        None,
    );
}

pub(crate) fn build_transfer_summary_log(
    succeeded: &[String],
    failed: &[RemoteTransferFailedItem],
) -> String {
    let mut log = String::new();
    log.push_str(&format!("成功 {} 個\n", succeeded.len()));
    for path in succeeded {
        log.push_str("  ✓ ");
        log.push_str(path);
        log.push('\n');
    }
    log.push_str(&format!("\n失敗 {} 個\n", failed.len()));
    for item in failed {
        log.push_str("  ✗ ");
        log.push_str(&item.path);
        log.push_str(": ");
        log.push_str(&item.error);
        log.push('\n');
    }
    log
}

fn build_transfer_debug_log(
    host: &str,
    port: u16,
    selections: &[RemotePcTransferSelection],
    dest_tree_uri: &str,
    file_index: u32,
    file_count: u32,
    last_remote_path: &str,
    err_chain: &str,
) -> String {
    let mut log = format!(
        "=== 遠端傳輸除錯 ===\nhost={host}:{port}\ndest_tree_uri={dest_tree_uri}\nprogress={file_index}/{file_count}\n"
    );
    if !last_remote_path.is_empty() {
        log.push_str(&format!("last_remote_path={last_remote_path}\n"));
    }
    log.push_str("selections:\n");
    for (i, sel) in selections.iter().enumerate() {
        log.push_str(&format!(
            "  [{i}] path={} anchor={}\n",
            sel.path, sel.anchor_path
        ));
    }
    log.push_str("\nerror_chain:\n");
    log.push_str(err_chain);
    log
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn last_segment(path: &str) -> String {
    normalize_path(path)
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn parent_path(path: &str) -> String {
    let normalized = normalize_path(path);
    match normalized.rfind('/') {
        Some(0) => String::new(),
        Some(index) => normalized[..index].to_string(),
        None => String::new(),
    }
}

/// 手機寫入路徑：以勾選時所在目錄為基準，而非 PC 分享根目錄。
fn phone_dest_relative_path(file_rel: &str, selection_path: &str, anchor_path: &str) -> String {
    let file = normalize_path(file_rel);
    let sel = normalize_path(selection_path);
    let anchor = normalize_path(anchor_path);

    if file == sel {
        return last_segment(&sel);
    }

    let anchor_prefix = if anchor.is_empty() {
        String::new()
    } else {
        format!("{anchor}/")
    };

    if anchor_prefix.is_empty() {
        if let Some(rest) = file.strip_prefix(&(format!("{sel}/"))) {
            return format!("{}/{}", last_segment(&sel), rest);
        }
        return file;
    }

    let Some(tail) = file.strip_prefix(&anchor_prefix) else {
        return file;
    };

    let sel_parent = parent_path(&sel);
    if sel_parent != anchor {
        return tail.to_string();
    }

    let anchor_name = last_segment(&anchor);
    let sel_name = last_segment(&sel);

    // 從上層列表勾選整個子資料夾（如「未分類」下勾「未分類002」）
    if sel_name.starts_with(&anchor_name)
        && sel_name != anchor_name
        && tail.split('/').next() == Some(sel_name.as_str())
    {
        return tail.to_string();
    }

    // 進入資料夾後勾選其子項（如「未分類002」內勾「檔案」）
    format!("{anchor_name}/{tail}")
}

async fn download_remote_file(
    app: &AppHandle,
    host: &str,
    port: u16,
    relative_path: &str,
    dest: &Path,
    file_index: u32,
    file_count: u32,
    bytes_base: u64,
    bytes_total: u64,
) -> anyhow::Result<u64> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .context("建立 HTTP 用戶端失敗")?;
    // POST JSON 避免 GET ?path= 中 [ ] # 等字元被 URL 解析截斷（如 [無修正].zip）
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/download"))
        .json(&json!({ "path": relative_path }))
        .send()
        .await
        .with_context(|| format!("下載失敗：{relative_path}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "下載 {relative_path} 失敗 HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("建立暫存檔失敗：{}", dest.display()))?;
    let mut stream = resp.bytes_stream();
    let mut file_bytes = 0_u64;
    let mut last_emit = Instant::now();
    let mut window_bytes = 0_u64;
    let window_start = Instant::now();
    while let Some(chunk) = stream.next().await {
        if remote_transfer_cancelled() {
            anyhow::bail!("傳輸已取消");
        }
        let chunk = chunk.with_context(|| format!("讀取 {relative_path} 串流失敗"))?;
        file.write_all(&chunk).await?;
        file_bytes += chunk.len() as u64;
        window_bytes += chunk.len() as u64;
        let now = Instant::now();
        if now.duration_since(last_emit) >= Duration::from_millis(250) {
            let elapsed = window_start.elapsed().as_secs_f64().max(0.001);
            let speed = (window_bytes as f64 / elapsed) as u64;
            emit_progress_active(
                app,
                "downloading",
                file_index,
                file_count,
                bytes_base + file_bytes,
                bytes_total,
                speed,
                relative_path,
                false,
                None,
                None,
            );
            last_emit = now;
        }
    }
    file.flush().await?;
    Ok(file_bytes)
}

#[cfg(not(target_os = "android"))]
pub async fn transfer_remote_pc_files(
    _app: &AppHandle,
    _host: &str,
    _port: u16,
    _selections: &[RemotePcTransferSelection],
    _dest_tree_uri: &str,
) -> anyhow::Result<()> {
    anyhow::bail!("遠端傳輸僅支援 Android")
}

#[cfg(target_os = "android")]
pub async fn transfer_remote_pc_files(
    app: &AppHandle,
    host: &str,
    port: u16,
    selections: &[RemotePcTransferSelection],
    dest_tree_uri: &str,
) -> anyhow::Result<()> {
    ensure_pc_remote_api_v2(host, port).await?;

    reset_remote_transfer_cancel();

    tracing::info!(
        host,
        port,
        selection_count = selections.len(),
        "remote_pc_transfer: start"
    );
    let mut last_remote_path = String::new();
    let mut progress_index = 0_u32;
    let mut progress_total = 0_u32;

    match transfer_remote_pc_files_inner(
        app,
        host,
        port,
        selections,
        dest_tree_uri,
        &mut last_remote_path,
        &mut progress_index,
        &mut progress_total,
    )
    .await
    {
        Ok(outcome) => {
            let summary_log = build_transfer_summary_log(&outcome.succeeded, &outcome.failed);
            let cancelled = remote_transfer_cancelled();
            let message = if cancelled {
                format!(
                    "已取消（已完成 {} / {} 個檔案）",
                    outcome.succeeded.len(),
                    progress_total
                )
            } else if outcome.failed.is_empty() {
                format!("傳輸完成（{} 個檔案）", outcome.succeeded.len())
            } else if outcome.succeeded.is_empty() {
                format!("傳輸失敗（{} 個檔案皆失敗）", outcome.failed.len())
            } else {
                format!(
                    "傳輸完成：成功 {} 個，失敗 {} 個",
                    outcome.succeeded.len(),
                    outcome.failed.len()
                )
            };
            let phase = if cancelled {
                "cancelled"
            } else if outcome.succeeded.is_empty() && !outcome.failed.is_empty() {
                "error"
            } else if outcome.failed.is_empty() {
                "done"
            } else {
                "partial"
            };
            let error = if !cancelled && outcome.succeeded.is_empty() && !outcome.failed.is_empty() {
                Some(message.clone())
            } else {
                None
            };
            emit_progress(
                app,
                phase,
                progress_index,
                progress_total,
                outcome.bytes_done,
                outcome.bytes_total,
                outcome.speed_bps,
                &message,
                true,
                error,
                Some(summary_log),
                Some(outcome.succeeded),
                Some(outcome.failed),
            );
            Ok(())
        }
        Err(err) => {
            let chain = err.to_string_chain();
            let detail = build_transfer_debug_log(
                host,
                port,
                selections,
                dest_tree_uri,
                progress_index,
                progress_total,
                &last_remote_path,
                &chain,
            );
            tracing::error!(detail = %detail, "remote_pc_transfer: failed");
            emit_progress(
                app,
                "error",
                progress_index,
                progress_total,
                0,
                0,
                0,
                &chain,
                true,
                Some(chain.clone()),
                Some(detail),
                None,
                None,
            );
            Err(err)
        }
    }
}

struct TransferOutcome {
    succeeded: Vec<String>,
    failed: Vec<RemoteTransferFailedItem>,
    bytes_done: u64,
    bytes_total: u64,
    speed_bps: u64,
}

#[cfg(target_os = "android")]
async fn transfer_remote_pc_files_inner(
    app: &AppHandle,
    host: &str,
    port: u16,
    selections: &[RemotePcTransferSelection],
    dest_tree_uri: &str,
    last_remote_path: &mut String,
    progress_index: &mut u32,
    progress_total: &mut u32,
) -> anyhow::Result<TransferOutcome> {
    let picker = folder_picker(app).map_err(|e| anyhow::anyhow!("{}", e.err_message))?;

    emit_progress_active(
        app,
        "collecting",
        0,
        0,
        0,
        0,
        0,
        "正在統計檔案…",
        false,
        None,
        None,
    );

    struct TransferFile {
        remote_path: String,
        size: u64,
        selection_path: String,
        anchor_path: String,
    }

    let mut files: Vec<TransferFile> = Vec::new();
    let mut collection_failed: Vec<RemoteTransferFailedItem> = Vec::new();
    for selection in selections {
        tracing::info!(
            path = %selection.path,
            anchor = %selection.anchor_path,
            "remote_pc_transfer: list files"
        );
        match list_remote_pc_files(host, port, &selection.path).await {
            Ok(list) => {
                for item in list {
                    files.push(TransferFile {
                        remote_path: item.relative_path,
                        size: item.size,
                        selection_path: selection.path.clone(),
                        anchor_path: selection.anchor_path.clone(),
                    });
                }
            }
            Err(err) => {
                let chain = err.to_string_chain();
                tracing::warn!(
                    path = %selection.path,
                    error = %chain,
                    "remote_pc_transfer: list files failed, skip selection"
                );
                collection_failed.push(RemoteTransferFailedItem {
                    path: selection.path.clone(),
                    error: format!("列出 PC 檔案失敗：{chain}"),
                });
            }
        }
    }
    files.sort_by(|a, b| a.remote_path.cmp(&b.remote_path));
    files.dedup_by(|a, b| a.remote_path == b.remote_path);
    if files.is_empty() {
        if collection_failed.is_empty() {
            anyhow::bail!("沒有可傳輸的檔案");
        }
        anyhow::bail!(
            "無法取得任何檔案清單：{}",
            collection_failed
                .iter()
                .map(|f| format!("{} ({})", f.path, f.error))
                .collect::<Vec<_>>()
                .join("；")
        );
    }

    let file_count = files.len() as u32;
    *progress_total = file_count;
    let bytes_total: u64 = files.iter().map(|f| f.size).sum();
    tracing::info!(file_count, bytes_total, "remote_pc_transfer: file list ready");

    let cache = app
        .path()
        .cache_dir()
        .context("取得快取目錄失敗")?;
    let temp_dir = cache.join(format!("gm-remote-transfer-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&temp_dir).await?;

    let mut bytes_done = 0_u64;
    let transfer_start = Instant::now();
    let mut succeeded: Vec<String> = Vec::new();
    let mut failed: Vec<RemoteTransferFailedItem> = collection_failed;

    for (index, file) in files.iter().enumerate() {
        if remote_transfer_cancelled() {
            break;
        }
        let file_index = index as u32 + 1;
        *progress_index = file_index;
        *last_remote_path = file.remote_path.clone();
        let temp_file = temp_dir.join(format!("{index}"));
        emit_progress_active(
            app,
            "downloading",
            file_index,
            file_count,
            bytes_done,
            bytes_total,
            0,
            &file.remote_path,
            false,
            None,
            None,
        );

        let transfer_result: anyhow::Result<u64> = async {
            let downloaded = download_remote_file(
                app,
                host,
                port,
                &file.remote_path,
                &temp_file,
                file_index,
                file_count,
                bytes_done,
                bytes_total,
            )
            .await
            .with_context(|| format!("下載失敗：{}", file.remote_path))?;

            let dest_rel = phone_dest_relative_path(
                &file.remote_path,
                &file.selection_path,
                &file.anchor_path,
            );
            emit_progress_active(
                app,
                "writing",
                file_index,
                file_count,
                bytes_done + downloaded,
                bytes_total,
                0,
                &format!("寫入手機：{dest_rel}"),
                false,
                None,
                None,
            );
            tracing::info!(
                remote = %file.remote_path,
                dest_rel = %dest_rel,
                file_index,
                file_count,
                "remote_pc_transfer: write to phone"
            );
            let source = temp_file
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("暫存路徑無效"))?;
            picker
                .copy_file_to_tree(dest_tree_uri, source, &dest_rel)
                .map_err(|e| anyhow::anyhow!("寫入手機失敗（{dest_rel}）：{}", e.err_message))?;
            Ok(downloaded)
        }
        .await;

        let _ = tokio::fs::remove_file(&temp_file).await;

        match transfer_result {
            Ok(downloaded) => {
                bytes_done += downloaded;
                succeeded.push(file.remote_path.clone());
            }
            Err(err) => {
                let chain = err.to_string_chain();
                if remote_transfer_cancelled() || chain.contains("已取消") {
                    break;
                }
                tracing::warn!(
                    path = %file.remote_path,
                    error = %chain,
                    "remote_pc_transfer: file failed, continue"
                );
                emit_progress_active(
                    app,
                    "skipped",
                    file_index,
                    file_count,
                    bytes_done,
                    bytes_total,
                    0,
                    &format!("跳過：{}", file.remote_path),
                    false,
                    None,
                    None,
                );
                failed.push(RemoteTransferFailedItem {
                    path: file.remote_path.clone(),
                    error: chain,
                });
            }
        }

        let elapsed = transfer_start.elapsed().as_secs_f64().max(0.001);
        let speed = (bytes_done as f64 / elapsed) as u64;
        emit_progress_active(
            app,
            "writing",
            file_index,
            file_count,
            bytes_done,
            bytes_total,
            speed,
            &format!(
                "進度 {file_index}/{file_count}（成功 {}，失敗 {}）",
                succeeded.len(),
                failed.len()
            ),
            false,
            None,
            None,
        );
    }

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    let elapsed = transfer_start.elapsed().as_secs_f64().max(0.001);
    let speed = (bytes_done as f64 / elapsed) as u64;
    tracing::info!(
        bytes_done,
        file_count,
        succeeded = succeeded.len(),
        failed = failed.len(),
        "remote_pc_transfer: batch done"
    );
    Ok(TransferOutcome {
        succeeded,
        failed,
        bytes_done,
        bytes_total,
        speed_bps: speed,
    })
}
