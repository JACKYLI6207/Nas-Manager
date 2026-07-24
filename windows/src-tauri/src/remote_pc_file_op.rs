use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::pc_remote_discovery::{ensure_pc_remote_api_v4, fetch_remote_pc_api_version};
use crate::remote_pc_transfer::RemoteTransferProgressEvent;

static REMOTE_FILE_OP_CANCEL: AtomicBool = AtomicBool::new(false);

const PROGRESS_CONNECT_RETRIES: u32 = 30;

pub fn reset_remote_file_op_cancel() {
    REMOTE_FILE_OP_CANCEL.store(false, Ordering::Relaxed);
}

pub fn request_remote_file_op_cancel() {
    REMOTE_FILE_OP_CANCEL.store(true, Ordering::Relaxed);
}

fn file_op_cancelled() -> bool {
    REMOTE_FILE_OP_CANCEL.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcFileOpResult {
    pub ok: bool,
    pub message: String,
    pub clipboard_count: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOpRequest {
    action: String,
    paths: Vec<String>,
    dest_path: String,
    new_name: String,
    #[serde(rename = "async")]
    async_op: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpResponse {
    ok: bool,
    message: String,
    clipboard_count: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpProgressBody {
    progress: Option<FileOpProgressSnapshot>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpProgressSnapshot {
    phase: String,
    file_index: u32,
    file_count: u32,
    bytes_done: u64,
    bytes_total: u64,
    speed_bps: u64,
    message: String,
    finished: bool,
    error: Option<String>,
}

fn emit_paste_progress(
    app: &AppHandle,
    snap: &FileOpProgressSnapshot,
    finished: bool,
    error: Option<String>,
) {
    let _ = RemoteTransferProgressEvent {
        phase: snap.phase.clone(),
        file_index: snap.file_index,
        file_count: snap.file_count,
        bytes_done: snap.bytes_done,
        bytes_total: snap.bytes_total,
        speed_bps: snap.speed_bps,
        message: snap.message.clone(),
        finished,
        error,
        detail_log: None,
        succeeded_paths: None,
        failed_items: None,
    }
    .emit(app);
}

async fn fetch_paste_progress(
    client: &reqwest::Client,
    host: &str,
    port: u16,
) -> anyhow::Result<FileOpProgressBody> {
    let url = format!("http://{host}:{port}/api/v1/file-op/progress");
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..PROGRESS_CONNECT_RETRIES {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.json().await.context("解析貼上進度失敗");
            }
            Ok(resp) => {
                anyhow::bail!("讀取貼上進度 HTTP {}", resp.status());
            }
            Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
                last_err = Some(e.into());
                let backoff_ms = 400u64 * (attempt as u64 + 1).min(5);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(e) => {
                return Err(anyhow::Error::from(e)).context("讀取貼上進度失敗");
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("連線失敗"))).context(
        "讀取貼上進度失敗：PC 連線中斷。若 Nas Manager 已閃退或關閉，請重新啟動 PC 版後再試",
    )
}

pub async fn remote_pc_file_op(
    host: &str,
    port: u16,
    action: &str,
    paths: &[String],
    dest_path: &str,
    new_name: &str,
) -> anyhow::Result<RemotePcFileOpResult> {
    ensure_pc_remote_api_v4(host, port).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("建立 HTTP 用戶端失敗")?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/file-op"))
        .json(&FileOpRequest {
            action: action.to_string(),
            paths: paths.to_vec(),
            dest_path: dest_path.to_string(),
            new_name: new_name.to_string(),
            async_op: false,
        })
        .send()
        .await
        .context("PC 檔案操作請求失敗")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "PC 檔案操作 HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let body: FileOpResponse = resp.json().await.context("解析 PC 檔案操作回應失敗")?;
    if !body.ok {
        anyhow::bail!("{}", body.message);
    }
    Ok(RemotePcFileOpResult {
        ok: body.ok,
        message: body.message,
        clipboard_count: body.clipboard_count,
    })
}

pub async fn remote_pc_paste_with_progress(
    app: &AppHandle,
    host: &str,
    port: u16,
    dest_path: &str,
) -> anyhow::Result<RemotePcFileOpResult> {
    ensure_pc_remote_api_v4(host, port).await?;
    let api = fetch_remote_pc_api_version(host, port).await;
    if api.unwrap_or(0) < 10 {
        anyhow::bail!(
            "PC 遠端服務不支援貼上進度（remote_api={}）。請更新 PC 版 EXE 並重新啟動遠端管理",
            api.unwrap_or(0)
        );
    }

    reset_remote_file_op_cancel();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(2)
        .build()
        .context("建立 HTTP 用戶端失敗")?;

    let resp = client
        .post(format!("http://{host}:{port}/api/v1/file-op"))
        .json(&FileOpRequest {
            action: "paste".to_string(),
            paths: vec![],
            dest_path: dest_path.to_string(),
            new_name: String::new(),
            async_op: true,
        })
        .send()
        .await
        .context("PC 貼上請求失敗")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "PC 貼上 HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let body: FileOpResponse = resp.json().await.context("解析 PC 貼上回應失敗")?;
    if !body.ok {
        anyhow::bail!("{}", body.message);
    }

    emit_paste_progress(
        app,
        &FileOpProgressSnapshot {
            phase: "scanning".to_string(),
            file_index: 0,
            file_count: 0,
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            message: "正在計算大小…".to_string(),
            finished: false,
            error: None,
        },
        false,
        None,
    );

    let mut retry_hint_sent = false;
    loop {
        if file_op_cancelled() {
            let _ = client
                .post(format!("http://{host}:{port}/api/v1/file-op/cancel"))
                .send()
                .await;
        }

        let prog_body = match fetch_paste_progress(&client, host, port).await {
            Ok(body) => {
                retry_hint_sent = false;
                body
            }
            Err(e) => {
                if !retry_hint_sent {
                    retry_hint_sent = true;
                    emit_paste_progress(
                        app,
                        &FileOpProgressSnapshot {
                            phase: "pasting".to_string(),
                            file_index: 0,
                            file_count: 0,
                            bytes_done: 0,
                            bytes_total: 0,
                            speed_bps: 0,
                            message: "PC 連線中斷，重試中…".to_string(),
                            finished: false,
                            error: None,
                        },
                        false,
                        None,
                    );
                }
                return Err(e);
            }
        };

        if let Some(snap) = prog_body.progress {
            let finished = snap.finished;
            let err = snap.error.clone();
            let message = snap.message.clone();
            let phase = snap.phase.clone();
            emit_paste_progress(app, &snap, finished, err.clone());
            if finished {
                if phase == "cancelled" || err.as_deref() == Some("已取消") {
                    return Ok(RemotePcFileOpResult {
                        ok: true,
                        message,
                        clipboard_count: None,
                    });
                }
                if let Some(e) = err {
                    anyhow::bail!("{e}");
                }
                return Ok(RemotePcFileOpResult {
                    ok: true,
                    message,
                    clipboard_count: None,
                });
            }
        } else if file_op_cancelled() {
            emit_paste_progress(
                app,
                &FileOpProgressSnapshot {
                    phase: "cancelled".to_string(),
                    file_index: 0,
                    file_count: 0,
                    bytes_done: 0,
                    bytes_total: 0,
                    speed_bps: 0,
                    message: "貼上已取消".to_string(),
                    finished: true,
                    error: Some("已取消".to_string()),
                },
                true,
                Some("已取消".to_string()),
            );
            return Ok(RemotePcFileOpResult {
                ok: true,
                message: "貼上已取消".to_string(),
                clipboard_count: None,
            });
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub fn cancel_remote_pc_file_op() {
    request_remote_file_op_cancel();
}
