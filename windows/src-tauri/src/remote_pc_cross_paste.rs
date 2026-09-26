//! Cross-PC paste client: only commands dest PC; transfer is dest<-source.

use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::pc_remote_discovery::fetch_remote_pc_api_version;
use crate::remote_pc_file_op::{
    is_remote_file_op_cancelled, reset_remote_file_op_cancel, RemotePcFileOpResult,
};
use crate::remote_pc_transfer::{RemotePcTransferSelection, RemoteTransferProgressEvent};

const PROGRESS_CONNECT_RETRIES: u32 = 30;
const MIN_CROSS_PASTE_API: u32 = 13;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CrossPasteRequest {
    action: String,
    paths: Vec<String>,
    dest_path: String,
    new_name: String,
    #[serde(rename = "async")]
    async_op: bool,
    source_host: String,
    source_port: u16,
    mode: String,
    selections: Vec<CrossPasteSelectionBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CrossPasteSelectionBody {
    path: String,
    anchor_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpResponse {
    ok: bool,
    message: String,
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
    #[serde(default)]
    log_line: Option<String>,
    #[serde(default)]
    detail_log: String,
}

fn emit_progress(
    app: &AppHandle,
    snap: &FileOpProgressSnapshot,
    finished: bool,
    error: Option<String>,
) {
    let detail = if !snap.detail_log.is_empty() {
        Some(snap.detail_log.clone())
    } else {
        snap.log_line.clone()
    };
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
        detail_log: detail,
        succeeded_paths: None,
        failed_items: None,
    }
    .emit(app);
}

async fn fetch_progress(
    client: &reqwest::Client,
    host: &str,
    port: u16,
) -> anyhow::Result<FileOpProgressBody> {
    let url = format!("http://{host}:{port}/api/v1/file-op/progress");
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..PROGRESS_CONNECT_RETRIES {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.json().await.context("parse paste progress failed");
            }
            Ok(resp) => {
                anyhow::bail!("paste progress HTTP {}", resp.status());
            }
            Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
                last_err = Some(e.into());
                let backoff_ms = 400u64 * (attempt as u64 + 1).min(5);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(e) => {
                return Err(anyhow::Error::from(e)).context("paste progress request failed");
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("connect failed"))).context(
        "paste progress failed: dest PC disconnected; ensure both PCs run remote management",
    )
}

pub async fn remote_pc_cross_paste_with_progress(
    app: &AppHandle,
    source_host: &str,
    source_port: u16,
    selections: &[RemotePcTransferSelection],
    dest_host: &str,
    dest_port: u16,
    dest_path: &str,
    mode: &str,
) -> anyhow::Result<RemotePcFileOpResult> {
    let api = fetch_remote_pc_api_version(dest_host, dest_port).await;
    if api.unwrap_or(0) < MIN_CROSS_PASTE_API {
        anyhow::bail!(
            "dest PC does not support cross-PC paste (remote_api={}, need >= {MIN_CROSS_PASTE_API}). Update both PC EXEs and restart remote management",
            api.unwrap_or(0)
        );
    }

    reset_remote_file_op_cancel();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(2)
        .build()
        .context("build HTTP client failed")?;

    let resp = client
        .post(format!("http://{dest_host}:{dest_port}/api/v1/file-op"))
        .json(&CrossPasteRequest {
            action: "cross-paste".to_string(),
            paths: selections.iter().map(|s| s.path.clone()).collect(),
            dest_path: dest_path.to_string(),
            new_name: String::new(),
            async_op: true,
            source_host: source_host.to_string(),
            source_port,
            mode: mode.to_string(),
            selections: selections
                .iter()
                .map(|s| CrossPasteSelectionBody {
                    path: s.path.clone(),
                    anchor_path: s.anchor_path.clone(),
                })
                .collect(),
        })
        .send()
        .await
        .context("cross-PC paste request failed")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "cross-PC paste HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!(": {body}")
            }
        );
    }
    let body: FileOpResponse = resp.json().await.context("parse cross-PC paste response failed")?;
    if !body.ok {
        anyhow::bail!("{}", body.message);
    }

    emit_progress(
        app,
        &FileOpProgressSnapshot {
            phase: "scanning".to_string(),
            file_index: 0,
            file_count: 0,
            bytes_done: 0,
            bytes_total: 0,
            speed_bps: 0,
            message: "scanning size...".to_string(),
            finished: false,
            error: None,
            log_line: None,
            detail_log: String::new(),
        },
        false,
        None,
    );

    loop {
        if is_remote_file_op_cancelled() {
            let _ = client
                .post(format!("http://{dest_host}:{dest_port}/api/v1/file-op/cancel"))
                .send()
                .await;
        }

        let prog_body = fetch_progress(&client, dest_host, dest_port).await?;
        if let Some(snap) = prog_body.progress {
            let finished = snap.finished;
            let err = snap.error.clone();
            let message = snap.message.clone();
            let phase = snap.phase.clone();
            emit_progress(app, &snap, finished, err.clone());
            if finished {
                if phase == "cancelled" || err.as_deref() == Some("???") {
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
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
