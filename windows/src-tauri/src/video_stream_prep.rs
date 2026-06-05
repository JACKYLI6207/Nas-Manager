use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Wry};
use tauri_specta::Event;

/// 預載目標：足夠測速並暖機，又不至於下載整支大檔。
const PREFETCH_TARGET_BYTES: u64 = 8 * 1024 * 1024;
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);
const STREAM_PREP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VideoStreamPrepProgressEvent {
    pub phase: String,
    pub message: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// 下載速率（位元組／秒）
    pub speed_bps: u64,
    pub finished: bool,
    pub error: Option<String>,
}

pub fn emit_video_stream_prep(app: &AppHandle<Wry>, event: VideoStreamPrepProgressEvent) {
    let _ = event.emit(app);
}

fn emit_phase(
    app: &AppHandle<Wry>,
    phase: &str,
    message: &str,
    bytes_done: u64,
    bytes_total: u64,
    speed_bps: u64,
    finished: bool,
    error: Option<String>,
) {
    emit_video_stream_prep(
        app,
        VideoStreamPrepProgressEvent {
            phase: phase.to_string(),
            message: message.to_string(),
            bytes_done,
            bytes_total,
            speed_bps,
            finished,
            error,
        },
    );
}

async fn probe_content_length(client: &reqwest::Client, stream_url: &str) -> Option<u64> {
    if let Ok(resp) = client.head(stream_url).send().await {
        if resp.status().is_success() {
            if let Some(len) = resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
            {
                return Some(len);
            }
        }
    }
    None
}

/// 預載串流開頭資料並回報進度；完成後再由外部啟動播放器。
pub async fn prefetch_stream_for_playback(
    app: &AppHandle<Wry>,
    stream_url: &str,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(STREAM_PREP_TIMEOUT)
        .build()
        .context("建立 HTTP 用戶端失敗")?;

    emit_phase(
        app,
        "buffering",
        "正在取得影片資訊…",
        0,
        0,
        0,
        false,
        None,
    );

    let file_size = probe_content_length(&client, stream_url).await;
    let prefetch_target = file_size
        .map(|size| size.min(PREFETCH_TARGET_BYTES))
        .unwrap_or(PREFETCH_TARGET_BYTES)
        .max(1);

    emit_phase(
        app,
        "buffering",
        "正在預載串流資料…",
        0,
        prefetch_target,
        0,
        false,
        None,
    );

    let range_end = prefetch_target.saturating_sub(1);
    let resp = client
        .get(stream_url)
        .header(reqwest::header::RANGE, format!("bytes=0-{range_end}"))
        .send()
        .await
        .context("無法連線至串流")?;

    if !(resp.status().is_success() || resp.status() == reqwest::StatusCode::PARTIAL_CONTENT) {
        anyhow::bail!("串流回應異常：HTTP {}", resp.status());
    }

    let start = Instant::now();
    let mut last_emit = Instant::now();
    let mut bytes_done = 0u64;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("讀取串流資料失敗")?;
        bytes_done += chunk.len() as u64;
        let now = Instant::now();
        if now.duration_since(last_emit) >= PROGRESS_EMIT_INTERVAL {
            last_emit = now;
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed_bps = (bytes_done as f64 / elapsed) as u64;
            emit_phase(
                app,
                "buffering",
                "正在預載串流資料…",
                bytes_done,
                prefetch_target,
                speed_bps,
                false,
                None,
            );
        }
    }

    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let speed_bps = (bytes_done as f64 / elapsed) as u64;
    emit_phase(
        app,
        "buffering",
        "預載完成",
        bytes_done,
        prefetch_target,
        speed_bps,
        false,
        None,
    );

    Ok(())
}
