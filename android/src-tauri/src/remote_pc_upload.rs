use std::time::{Duration, Instant};

use anyhow::Context;
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::io::AsyncReadExt;

use crate::extensions::AnyhowErrorToStringChain;
use crate::pc_remote_discovery::{check_remote_upload_conflicts, ensure_pc_remote_api_v3};
#[cfg(target_os = "android")]
use crate::folder_picker::folder_picker;
use crate::remote_pc_transfer::{
    build_transfer_summary_log, emit_progress_active, remote_transfer_cancelled,
    reset_remote_transfer_cancel, RemoteTransferFailedItem, RemoteTransferProgressEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUploadPlanItem {
    pub source_uri: String,
    pub dest_relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUploadPlan {
    pub files: Vec<RemoteUploadPlanItem>,
    pub conflicts: Vec<String>,
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn join_rel_path(base: &str, tail: &str) -> String {
    let base = normalize_path(base);
    let tail = normalize_path(tail);
    if base.is_empty() {
        tail
    } else if tail.is_empty() {
        base
    } else {
        format!("{base}/{tail}")
    }
}

#[cfg(not(target_os = "android"))]
pub async fn plan_remote_pc_upload(
    _app: &AppHandle,
    _host: &str,
    _port: u16,
    _pc_dest_dir: &str,
    _source_uri: &str,
    _kind: &str,
) -> anyhow::Result<RemoteUploadPlan> {
    anyhow::bail!("遠端上傳僅支援 Android")
}

#[cfg(target_os = "android")]
pub async fn plan_remote_pc_upload(
    app: &AppHandle,
    host: &str,
    port: u16,
    pc_dest_dir: &str,
    source_uri: &str,
    kind: &str,
) -> anyhow::Result<RemoteUploadPlan> {
    ensure_pc_remote_api_v3(host, port).await?;
    let picker = folder_picker(app).map_err(|e| anyhow::anyhow!("{}", e.err_message))?;
    let sources = picker
        .list_upload_files(source_uri, if kind == "folder" { "tree" } else { kind })
        .map_err(|e| anyhow::anyhow!("{}", e.err_message))?;
    if sources.is_empty() {
        anyhow::bail!("沒有可上傳的檔案");
    }
    let mut files = Vec::new();
    let mut dest_paths = Vec::new();
    for src in sources {
        let dest = join_rel_path(pc_dest_dir, &src.relative_path);
        dest_paths.push(dest.clone());
        files.push(RemoteUploadPlanItem {
            source_uri: src.uri,
            dest_relative_path: dest,
            size: src.size,
        });
    }
    let conflicts = check_remote_upload_conflicts(host, port, &dest_paths).await?;
    Ok(RemoteUploadPlan { files, conflicts })
}

#[cfg(not(target_os = "android"))]
pub async fn upload_remote_pc_files(
    _app: &AppHandle,
    _host: &str,
    _port: u16,
    _files: Vec<RemoteUploadPlanItem>,
    _on_conflict: &str,
) -> anyhow::Result<()> {
    anyhow::bail!("遠端上傳僅支援 Android")
}

#[cfg(target_os = "android")]
pub async fn upload_remote_pc_files(
    app: &AppHandle,
    host: &str,
    port: u16,
    files: Vec<RemoteUploadPlanItem>,
    on_conflict: &str,
) -> anyhow::Result<()> {
    ensure_pc_remote_api_v3(host, port).await?;
    reset_remote_transfer_cancel();
    if files.is_empty() {
        anyhow::bail!("沒有可上傳的檔案");
    }
    let picker = folder_picker(app).map_err(|e| anyhow::anyhow!("{}", e.err_message))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .context("建立 HTTP 用戶端失敗")?;

    let file_count = files.len() as u32;
    let bytes_total: u64 = files.iter().map(|f| f.size).sum();
    let mut bytes_done = 0_u64;
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    let upload_start = Instant::now();

    emit_progress_active(
        app,
        "collecting",
        0,
        file_count,
        0,
        bytes_total,
        0,
        "準備上傳…",
        false,
        None,
        None,
    );

    for (index, file) in files.iter().enumerate() {
        if remote_transfer_cancelled() {
            break;
        }
        let file_index = index as u32 + 1;
        emit_progress_active(
            app,
            "uploading",
            file_index,
            file_count,
            bytes_done,
            bytes_total,
            0,
            &file.dest_relative_path,
            false,
            None,
            None,
        );

        let upload_result: anyhow::Result<u64> = async {
            emit_progress_active(
                app,
                "uploading",
                file_index,
                file_count,
                bytes_done,
                bytes_total,
                0,
                &format!("準備：{}", file.dest_relative_path),
                false,
                None,
                None,
            );

            let cache_path = picker
                .cache_document_to_file(&file.source_uri)
                .map_err(|e| anyhow::anyhow!("{}", e.err_message))?;
            let file_len = tokio::fs::metadata(&cache_path)
                .await
                .with_context(|| format!("讀取暫存檔大小失敗：{}", cache_path))?
                .len();
            let path_b64 = base64::engine::general_purpose::STANDARD
                .encode(file.dest_relative_path.as_bytes());

            struct UploadStreamState {
                file: tokio::fs::File,
                buf: Vec<u8>,
                file_base: u64,
                bytes_in_file: u64,
                last_emit: Instant,
                app: AppHandle,
                file_index: u32,
                file_count: u32,
                bytes_total: u64,
                upload_start: Instant,
                dest_path: String,
            }

            let file_handle = tokio::fs::File::open(&cache_path)
                .await
                .with_context(|| format!("開啟暫存檔失敗：{}", cache_path))?;
            let stream_state = UploadStreamState {
                file: file_handle,
                buf: vec![0u8; 65536],
                file_base: bytes_done,
                bytes_in_file: 0,
                last_emit: Instant::now(),
                app: app.clone(),
                file_index,
                file_count,
                bytes_total,
                upload_start,
                dest_path: file.dest_relative_path.clone(),
            };

            let body_stream = futures_util::stream::unfold(stream_state, |mut st| async move {
                if remote_transfer_cancelled() {
                    return Some((
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "傳輸已取消",
                        )),
                        st,
                    ));
                }
                match st.file.read(&mut st.buf).await {
                    Ok(0) => None,
                    Ok(n) => {
                        st.bytes_in_file += n as u64;
                        let now = Instant::now();
                        if now.duration_since(st.last_emit) >= Duration::from_millis(250) {
                            st.last_emit = now;
                            let elapsed = st.upload_start.elapsed().as_secs_f64().max(0.001);
                            let current = st.file_base + st.bytes_in_file;
                            let speed = (current as f64 / elapsed) as u64;
                            emit_progress_active(
                                &st.app,
                                "uploading",
                                st.file_index,
                                st.file_count,
                                current,
                                st.bytes_total,
                                speed,
                                &st.dest_path,
                                false,
                                None,
                                None,
                            );
                        }
                        let chunk = Bytes::copy_from_slice(&st.buf[..n]);
                        Some((Ok(chunk), st))
                    }
                    Err(e) => Some((
                        Err(std::io::Error::new(
                            e.kind(),
                            format!("讀取上傳檔失敗：{}", e),
                        )),
                        st,
                    )),
                }
            });

            let resp = client
                .post(format!("http://{host}:{port}/api/v1/upload"))
                .header("X-GM-Rel-Path-B64", path_b64)
                .header("X-GM-On-Conflict", on_conflict)
                .body(reqwest::Body::wrap_stream(body_stream))
                .send()
                .await
                .with_context(|| format!("上傳失敗：{}", file.dest_relative_path))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!(
                    "上傳 {} 失敗 HTTP {status}{}",
                    file.dest_relative_path,
                    if text.is_empty() {
                        String::new()
                    } else {
                        format!("：{text}")
                    }
                );
            }
            let _ = tokio::fs::remove_file(&cache_path).await;
            Ok(file_len)
        }
        .await;

        match upload_result {
            Ok(n) => {
                bytes_done += n;
                succeeded.push(file.dest_relative_path.clone());
            }
            Err(err) => {
                let chain = err.to_string_chain();
                if remote_transfer_cancelled() || chain.contains("已取消") {
                    break;
                }
                tracing::warn!(
                    path = %file.dest_relative_path,
                    error = %chain,
                    "remote_pc_upload: file failed, continue"
                );
                emit_progress_active(
                    app,
                    "skipped",
                    file_index,
                    file_count,
                    bytes_done,
                    bytes_total,
                    0,
                    &format!("跳過：{}", file.dest_relative_path),
                    false,
                    None,
                    None,
                );
                failed.push(RemoteTransferFailedItem {
                    path: file.dest_relative_path.clone(),
                    error: chain,
                });
            }
        }

        let elapsed = upload_start.elapsed().as_secs_f64().max(0.001);
        let speed = (bytes_done as f64 / elapsed) as u64;
        emit_progress_active(
            app,
            "uploading",
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

    let elapsed = upload_start.elapsed().as_secs_f64().max(0.001);
    let speed = (bytes_done as f64 / elapsed) as u64;
    let summary_log = build_transfer_summary_log(&succeeded, &failed);
    let cancelled = remote_transfer_cancelled();
    let message = if cancelled {
        format!(
            "已取消（已上傳 {} / {} 個檔案）",
            succeeded.len(),
            file_count
        )
    } else if failed.is_empty() {
        format!("上傳完成（{} 個檔案）", succeeded.len())
    } else if succeeded.is_empty() {
        format!("上傳失敗（{} 個檔案皆失敗）", failed.len())
    } else {
        format!(
            "上傳完成：成功 {} 個，失敗 {} 個",
            succeeded.len(),
            failed.len()
        )
    };
    let phase = if cancelled {
        "cancelled"
    } else if succeeded.is_empty() && !failed.is_empty() {
        "error"
    } else if failed.is_empty() {
        "done"
    } else {
        "partial"
    };
    let error = if !cancelled && succeeded.is_empty() && !failed.is_empty() {
        Some(message.clone())
    } else {
        None
    };
    let _ = RemoteTransferProgressEvent {
        phase: phase.to_string(),
        file_index: file_count,
        file_count,
        bytes_done,
        bytes_total,
        speed_bps: speed,
        message: message.clone(),
        finished: true,
        error,
        detail_log: Some(summary_log),
        succeeded_paths: Some(succeeded),
        failed_items: Some(failed),
    }
    .emit(app);
    Ok(())
}
