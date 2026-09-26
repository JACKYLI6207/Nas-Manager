use std::net::Ipv4Addr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio_util::io::ReaderStream;

use crate::extensions::{AnyhowErrorToStringChain, AppHandleExt};
use crate::pc_remote_discovery::{
    check_remote_upload_conflicts, ensure_pc_remote_api_v3, fetch_remote_pc_api_version,
};
use crate::remote_pc_file_op::{
    is_remote_file_op_cancelled, reset_remote_file_op_cancel,
};
use crate::remote_pc_transfer::{
    build_transfer_summary_log, emit_progress_active, remote_transfer_cancelled,
    reset_remote_transfer_cancel, RemoteTransferFailedItem, RemoteTransferProgressEvent,
};
use crate::xfer_source::{self, XferSourceFile};

/// 與跨 PC 貼上拉檔同級：大塊讀取（舊 push 後備路徑用）。
const UPLOAD_READ_BUF: usize = 4 * 1024 * 1024;
/// 目的端支援 pull-upload（remote_api）
const MIN_PULL_UPLOAD_API: u32 = 14;
const PROGRESS_CONNECT_RETRIES: u32 = 30;

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

struct LocalUploadSource {
    local_path: String,
    relative_path: String,
    size: u64,
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

fn collect_local_upload_sources(source_uri: &str, kind: &str) -> anyhow::Result<Vec<LocalUploadSource>> {
    let source = Path::new(source_uri);
    if !source.exists() {
        anyhow::bail!("來源路徑不存在");
    }

    let mut out = Vec::new();
    if kind == "folder" || kind == "tree" {
        if !source.is_dir() {
            anyhow::bail!("請選擇資料夾");
        }
        let base = source.to_path_buf();
        collect_dir_upload_sources(&base, source, &mut out)?;
    } else {
        if !source.is_file() {
            anyhow::bail!("請選擇檔案");
        }
        let size = std::fs::metadata(source)?.len();
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(LocalUploadSource {
            local_path: source.display().to_string(),
            relative_path: name,
            size,
        });
    }
    Ok(out)
}

fn collect_dir_upload_sources(
    base: &Path,
    dir: &Path,
    out: &mut Vec<LocalUploadSource>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_upload_sources(base, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .components()
                .filter(|c| !matches!(c, Component::RootDir | Component::Prefix(_)))
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let size = std::fs::metadata(&path)?.len();
            out.push(LocalUploadSource {
                local_path: path.display().to_string(),
                relative_path: rel,
                size,
            });
        }
    }
    Ok(())
}

pub async fn plan_remote_pc_upload(
    _app: &AppHandle,
    host: &str,
    port: u16,
    pc_dest_dir: &str,
    source_uri: &str,
    kind: &str,
) -> anyhow::Result<RemoteUploadPlan> {
    ensure_pc_remote_api_v3(host, port).await?;
    let sources = collect_local_upload_sources(source_uri, kind)?;
    if sources.is_empty() {
        anyhow::bail!("沒有可上傳的檔案");
    }
    let mut files = Vec::new();
    let mut dest_paths = Vec::new();
    for src in sources {
        let dest = join_rel_path(pc_dest_dir, &src.relative_path);
        dest_paths.push(dest.clone());
        files.push(RemoteUploadPlanItem {
            source_uri: src.local_path,
            dest_relative_path: dest,
            size: src.size,
        });
    }
    let conflicts = check_remote_upload_conflicts(host, port, &dest_paths).await?;
    Ok(RemoteUploadPlan { files, conflicts })
}

fn parse_ipv4(host: &str) -> Option<Ipv4Addr> {
    let h = host.trim();
    let h = h.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(h);
    // 去掉可能的 :port（IPv4）
    let h = if let Some((ip, _)) = h.rsplit_once(':') {
        if ip.contains('.') {
            ip
        } else {
            h
        }
    } else {
        h
    };
    h.parse().ok()
}

/// 選一個目的端較可能連得上的本機 LAN IP（同 /24 優先，其次同 /16）。
fn pick_source_lan_ip(local_addrs: &[String], dest_host: &str) -> Option<String> {
    let dest_ip = parse_ipv4(dest_host)?;
    let dest_o = dest_ip.octets();
    let parsed: Vec<(String, [u8; 4])> = local_addrs
        .iter()
        .filter_map(|a| parse_ipv4(a).map(|ip| (a.clone(), ip.octets())))
        .collect();
    if parsed.is_empty() {
        return None;
    }
    if let Some((s, _)) = parsed.iter().find(|(_, o)| o[0] == dest_o[0] && o[1] == dest_o[1] && o[2] == dest_o[2])
    {
        return Some(s.clone());
    }
    if let Some((s, _)) = parsed.iter().find(|(_, o)| o[0] == dest_o[0] && o[1] == dest_o[1]) {
        return Some(s.clone());
    }
    Some(parsed[0].0.clone())
}

async fn ensure_local_remote_source(app: &AppHandle) -> anyhow::Result<(String, u16, Vec<String>)> {
    let config = app.get_config().read().clone();
    if !config.remote_management_enabled {
        anyhow::bail!("本機遠端管理未啟用");
    }
    if !crate::remote_management::is_running_with_config(&config) {
        crate::remote_management::sync_async(app).await;
    }
    let status = crate::remote_management::get_status(&config);
    if !status.running {
        let detail = status
            .last_error
            .unwrap_or_else(|| "未知原因".to_string());
        anyhow::bail!("本機遠端管理未在執行：{detail}");
    }
    Ok((
        String::new(), // host 稍後依目的端挑選
        status.port,
        status.lan_addresses,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullUploadRequest {
    action: String,
    #[serde(rename = "async")]
    async_op: bool,
    dest_path: String,
    source_host: String,
    source_port: u16,
    xfer_token: String,
    on_conflict: String,
    xfer_files: Vec<PullUploadFileBody>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullUploadFileBody {
    relative_path: String,
    size: u64,
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

fn emit_pull_progress(app: &AppHandle, snap: &FileOpProgressSnapshot, finished: bool, error: Option<String>) {
    let detail = if !snap.detail_log.is_empty() {
        Some(snap.detail_log.clone())
    } else {
        snap.log_line.clone()
    };
    // 統一成 uploading，避免 UI 顯示成貼上
    let phase = if snap.phase == "pasting" || snap.phase == "scanning" {
        if finished {
            snap.phase.clone()
        } else if snap.phase == "scanning" {
            "uploading".to_string()
        } else {
            "uploading".to_string()
        }
    } else if snap.phase.is_empty() {
        "uploading".to_string()
    } else {
        snap.phase.clone()
    };
    let _ = RemoteTransferProgressEvent {
        phase,
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

async fn fetch_file_op_progress(
    client: &reqwest::Client,
    host: &str,
    port: u16,
) -> anyhow::Result<FileOpProgressBody> {
    let url = format!("http://{host}:{port}/api/v1/file-op/progress");
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..PROGRESS_CONNECT_RETRIES {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp.json().await.context("解析上傳進度失敗");
            }
            Ok(resp) => {
                anyhow::bail!("上傳進度 HTTP {}", resp.status());
            }
            Err(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
                last_err = Some(e.into());
                let backoff_ms = 400u64 * (attempt as u64 + 1).min(5);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
            Err(e) => {
                return Err(anyhow::Error::from(e)).context("上傳進度請求失敗");
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("連線失敗"))).context(
        "上傳進度失敗：目的 PC 斷線；請確認兩邊遠端管理皆在執行",
    )
}

async fn upload_via_dest_pull(
    app: &AppHandle,
    dest_host: &str,
    dest_port: u16,
    files: &[RemoteUploadPlanItem],
    on_conflict: &str,
) -> anyhow::Result<()> {
    let (_ignored, source_port, lan_addrs) = ensure_local_remote_source(app).await?;
    let source_host = pick_source_lan_ip(&lan_addrs, dest_host).ok_or_else(|| {
        anyhow::anyhow!("找不到可給目的 PC 連線的本機區網 IP")
    })?;

    let xfer_files: Vec<XferSourceFile> = files
        .iter()
        .map(|f| XferSourceFile {
            relative_path: f.dest_relative_path.clone(),
            absolute_path: PathBuf::from(&f.source_uri),
            size: f.size,
        })
        .collect();
    let token = xfer_source::register_session(xfer_files)?;

    reset_remote_transfer_cancel();
    reset_remote_file_op_cancel();

    let file_count = files.len() as u32;
    let bytes_total: u64 = files.iter().map(|f| f.size).sum();
    emit_progress_active(
        app,
        "uploading",
        0,
        file_count,
        0,
        bytes_total,
        0,
        &format!("目的端拉取模式：本機 {source_host}:{source_port} → {dest_host}:{dest_port}"),
        false,
        Some(format!(
            "上傳改為目的 PC 下載本機暫存（token={}…）",
            &token[..8.min(token.len())]
        )),
        None,
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(2)
        .tcp_nodelay(true)
        .build()
        .context("建立 HTTP 用戶端失敗")?;

    let req = PullUploadRequest {
        action: "pull-upload".to_string(),
        async_op: true,
        dest_path: String::new(),
        source_host: source_host.clone(),
        source_port,
        xfer_token: token.clone(),
        on_conflict: on_conflict.to_string(),
        xfer_files: files
            .iter()
            .map(|f| PullUploadFileBody {
                relative_path: f.dest_relative_path.clone(),
                size: f.size,
            })
            .collect(),
    };

    let start_result = async {
        let resp = client
            .post(format!("http://{dest_host}:{dest_port}/api/v1/file-op"))
            .json(&req)
            .send()
            .await
            .context("目的端拉取上傳請求失敗")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "目的端拉取上傳 HTTP {status}{}",
                if body.is_empty() {
                    String::new()
                } else {
                    format!("：{body}")
                }
            );
        }
        let body: FileOpResponse = resp.json().await.context("解析目的端拉取回應失敗")?;
        if !body.ok {
            anyhow::bail!("{}", body.message);
        }

        loop {
            if remote_transfer_cancelled() || is_remote_file_op_cancelled() {
                let _ = client
                    .post(format!("http://{dest_host}:{dest_port}/api/v1/file-op/cancel"))
                    .send()
                    .await;
            }

            let prog_body = fetch_file_op_progress(&client, dest_host, dest_port).await?;
            if let Some(snap) = prog_body.progress {
                let finished = snap.finished;
                let err = snap.error.clone();
                let message = snap.message.clone();
                let phase = snap.phase.clone();
                emit_pull_progress(app, &snap, finished, err.clone());
                if finished {
                    if phase == "cancelled" || message.contains("取消") {
                        return Ok(());
                    }
                    if let Some(e) = err {
                        anyhow::bail!("{e}");
                    }
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    .await;

    xfer_source::unregister_session(&token);
    start_result
}

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

    let dest_api = fetch_remote_pc_api_version(host, port).await.unwrap_or(0);
    if dest_api >= MIN_PULL_UPLOAD_API {
        match upload_via_dest_pull(app, host, port, &files, on_conflict).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let msg = e.to_string_chain();
                // 本機遠端管理不可用時退回舊 push；其他錯誤（含目的端失敗）直接回報
                if msg.contains("本機遠端管理") || msg.contains("本機區網 IP") {
                    emit_progress_active(
                        app,
                        "uploading",
                        0,
                        files.len() as u32,
                        0,
                        files.iter().map(|f| f.size).sum(),
                        0,
                        &format!("目的端拉取不可用，改用推送上傳：{msg}"),
                        false,
                        Some(format!("pull-upload fallback: {msg}")),
                        None,
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }

    upload_via_push(app, host, port, files, on_conflict).await
}

async fn upload_via_push(
    app: &AppHandle,
    host: &str,
    port: u16,
    files: Vec<RemoteUploadPlanItem>,
    on_conflict: &str,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .tcp_nodelay(true)
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
        "準備上傳（推送）…",
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
            let path_b64 = base64::engine::general_purpose::STANDARD
                .encode(file.dest_relative_path.as_bytes());
            let file_handle = tokio::fs::File::open(&file.source_uri)
                .await
                .with_context(|| format!("開啟檔案失敗：{}", file.source_uri))?;
            let file_len = tokio::fs::metadata(&file.source_uri).await?.len();

            let bytes_in_file = Arc::new(AtomicU64::new(0));
            let done_flag = Arc::new(AtomicBool::new(false));
            let progress_bytes = Arc::clone(&bytes_in_file);
            let progress_done = Arc::clone(&done_flag);
            let progress_app = app.clone();
            let dest_path = file.dest_relative_path.clone();
            let file_base = bytes_done;
            let progress_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(250));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if progress_done.load(Ordering::Relaxed) || remote_transfer_cancelled() {
                        break;
                    }
                    let current = file_base + progress_bytes.load(Ordering::Relaxed);
                    let elapsed = upload_start.elapsed().as_secs_f64().max(0.001);
                    let speed = (current as f64 / elapsed) as u64;
                    emit_progress_active(
                        &progress_app,
                        "uploading",
                        file_index,
                        file_count,
                        current,
                        bytes_total,
                        speed,
                        &dest_path,
                        false,
                        None,
                        None,
                    );
                }
            });

            let counter = Arc::clone(&bytes_in_file);
            let body_stream = ReaderStream::with_capacity(file_handle, UPLOAD_READ_BUF).map(
                move |res| match res {
                    Ok(chunk) => {
                        if remote_transfer_cancelled() {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                "傳輸已取消",
                            ));
                        }
                        counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        Ok(chunk)
                    }
                    Err(e) => Err(std::io::Error::new(
                        e.kind(),
                        format!("讀取上傳檔失敗：{e}"),
                    )),
                },
            );

            let send_result = client
                .post(format!("http://{host}:{port}/api/v1/upload"))
                .header("X-GM-Rel-Path-B64", path_b64)
                .header("X-GM-On-Conflict", on_conflict)
                .header("Content-Length", file_len.to_string())
                .body(reqwest::Body::wrap_stream(body_stream))
                .send()
                .await;

            done_flag.store(true, Ordering::Relaxed);
            let _ = progress_task.await;

            let resp = send_result.with_context(|| format!("上傳失敗：{}", file.dest_relative_path))?;
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
                failed.push(RemoteTransferFailedItem {
                    path: file.dest_relative_path.clone(),
                    error: chain,
                });
            }
        }
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
