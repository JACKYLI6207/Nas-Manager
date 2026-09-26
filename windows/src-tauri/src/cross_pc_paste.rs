//! 目標 PC 伺服端：從來源 PC HTTP 拉取檔案並寫入本機分享目錄（客戶端只下指令）。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use serde::Deserialize;

use crate::file_op_job::ProgressCtx;

#[derive(Clone, Debug)]
pub struct CrossPasteSelection {
    pub path: String,
    pub anchor_path: String,
}

#[derive(Deserialize)]
struct ListFilesResponse {
    ok: bool,
    #[serde(default)]
    files: Vec<RemoteFileItem>,
}

#[derive(Deserialize)]
struct RemoteFileItem {
    #[serde(rename = "relativePath")]
    relative_path: String,
    size: u64,
}

#[derive(Deserialize)]
struct FileOpResponseBody {
    ok: bool,
    #[serde(default)]
    message: String,
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

fn last_segment(path: &str) -> String {
    normalize_path(path)
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// 貼上路徑只相對「勾選項目」本身，不要帶上一層瀏覽目錄（anchor）。
/// 例：在 A 下勾選 B → 貼出 B/…，而非 A/B/…
fn paste_dest_relative_path(file_rel: &str, selection_path: &str, _anchor_path: &str) -> String {
    let file = normalize_path(file_rel);
    let sel = normalize_path(selection_path);
    if file == sel {
        return last_segment(&sel);
    }
    if let Some(rest) = file.strip_prefix(&format!("{sel}/")) {
        return format!("{}/{}", last_segment(&sel), rest);
    }
    last_segment(&file)
}

fn path_exists(path: &Path) -> bool {
    path.exists()
}

fn unique_dest_path(dest_dir: &Path, base_name: &str) -> PathBuf {
    let mut candidate = dest_dir.join(base_name);
    if !path_exists(&candidate) {
        return candidate;
    }
    let path = Path::new(base_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| base_name.to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..1000 {
        candidate = dest_dir.join(format!("{stem} ({n}){ext}"));
        if !path_exists(&candidate) {
            return candidate;
        }
    }
    dest_dir.join(format!("{stem}_copy{ext}"))
}

fn top_segment(rel: &str) -> String {
    let n = normalize_path(rel);
    n.split('/').next().unwrap_or("").to_string()
}

fn remap_top(rel: &str, old_top: &str, new_top: &str) -> String {
    let n = normalize_path(rel);
    if n == old_top {
        return new_top.to_string();
    }
    let prefix = format!("{old_top}/");
    if let Some(rest) = n.strip_prefix(&prefix) {
        return format!("{new_top}/{rest}");
    }
    n
}

struct PlannedFile {
    remote_path: String,
    dest_abs: PathBuf,
    size: u64,
}

fn list_source_files(
    client: &reqwest::blocking::Client,
    source_host: &str,
    source_port: u16,
    path: &str,
) -> anyhow::Result<Vec<RemoteFileItem>> {
    let url = format!("http://{source_host}:{source_port}/api/v1/list-files");
    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(30))
        .json(&serde_json::json!({ "path": path }))
        .send()
        .with_context(|| format!("向來源 PC 列檔失敗：{path}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("來源 PC 列檔 HTTP {}", resp.status());
    }
    let body: ListFilesResponse = resp.json().context("解析來源 PC 列檔回應失敗")?;
    if !body.ok {
        anyhow::bail!("來源 PC 列檔失敗");
    }
    Ok(body.files)
}

fn format_mbps(bytes: u64, secs: f64) -> String {
    if secs <= 0.0 {
        return "0 MB/s".to_string();
    }
    let mbps = (bytes as f64 / secs) / (1024.0 * 1024.0);
    format!("{mbps:.1} MB/s")
}

/// 同步大緩衝拉取並立刻寫碟（與同機貼上相同模型）。
/// 每秒輸出診斷 LOG：網路讀速／磁碟寫速／單次寫入耗時，方便抓「沒在寫入」的問題。
fn download_http_body_to(
    source_host: &str,
    source_port: u16,
    url: &str,
    req_json: serde_json::Value,
    label: &str,
    dest: &Path,
    ctx: &ProgressCtx,
) -> anyhow::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    ctx.push_log(&format!("開始拉取 {label} → {}", dest.display()));
    ctx.check_cancelled()?;

    let t_connect = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .tcp_nodelay(true)
        // 整段請求不設總逾時；靠循環內取消檢查
        .timeout(None)
        .build()
        .context("建立 HTTP 用戶端失敗")?;
    let mut resp = client
        .post(url)
        .json(&req_json)
        .send()
        .with_context(|| format!("從來源 PC 下載失敗：{label}"))?;
    let connect_ms = t_connect.elapsed().as_millis();
    if !resp.status().is_success() {
        anyhow::bail!("從來源 PC 下載 HTTP {}：{label}", resp.status());
    }
    let content_len = resp.content_length();
    ctx.push_log(&format!(
        "已連線來源 {source_host}:{source_port}（{connect_ms} ms）Content-Length={}",
        content_len
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string())
    ));

    let file = std::fs::File::create(dest).context("建立目的檔失敗")?;
    // 4MB 緩衝：滿了就真正寫入系統，避免看起來「磁碟沒在動」
    let mut outfile = std::io::BufWriter::with_capacity(4 * 1024 * 1024, file);
    let mut buf = vec![0u8; 4 * 1024 * 1024];

    let mut written = 0u64;
    let mut window_read_bytes = 0u64;
    let mut window_write_bytes = 0u64;
    let mut window_write_ms = 0u128;
    let mut window_read_ms = 0u128;
    let mut max_write_ms = 0u128;
    let mut last_log = Instant::now();
    let mut last_cancel_check = Instant::now();
    let transfer_start = Instant::now();
    let mut since_flush = 0u64;

    loop {
        if last_cancel_check.elapsed() >= Duration::from_millis(200) {
            ctx.check_cancelled()?;
            last_cancel_check = Instant::now();
        }

        let t_read = Instant::now();
        let n = resp
            .read(&mut buf)
            .with_context(|| format!("從來源 PC 讀取失敗：{label}"))?;
        let read_ms = t_read.elapsed().as_millis();
        if n == 0 {
            break;
        }
        window_read_ms += read_ms;
        window_read_bytes += n as u64;

        let t_write = Instant::now();
        outfile
            .write_all(&buf[..n])
            .context("寫入目的檔失敗")?;
        let write_ms = t_write.elapsed().as_millis();
        window_write_ms += write_ms;
        window_write_bytes += n as u64;
        max_write_ms = max_write_ms.max(write_ms);
        written += n as u64;
        since_flush += n as u64;
        ctx.add_bytes(n as u64);

        // 每累積約 8MB 強制 flush，讓工作管理員看得到磁碟寫入
        if since_flush >= 8 * 1024 * 1024 {
            let t_flush = Instant::now();
            outfile.flush().context("flush 目的檔失敗")?;
            let flush_ms = t_flush.elapsed().as_millis();
            since_flush = 0;
            if flush_ms >= 50 {
                ctx.push_log(&format!(
                    "警告：flush 耗時 {flush_ms} ms（磁碟可能忙碌或被防毒掃描）"
                ));
            }
        }

        if write_ms >= 200 {
            ctx.push_log(&format!(
                "警告：單次寫入 {n} bytes 耗時 {write_ms} ms"
            ));
        }

        if last_log.elapsed() >= Duration::from_secs(1) {
            let secs = last_log.elapsed().as_secs_f64().max(0.001);
            let read_spd = format_mbps(window_read_bytes, secs);
            let write_spd = format_mbps(window_write_bytes, secs);
            let overall = format_mbps(written, transfer_start.elapsed().as_secs_f64().max(0.001));
            let line = format!(
                "診斷 已傳 {:.1} MB | 讀 {read_spd}（讀耗時 {window_read_ms} ms）| 寫 {write_spd}（寫耗時 {window_write_ms} ms，單次最大 {max_write_ms} ms）| 平均 {overall}",
                written as f64 / (1024.0 * 1024.0),
            );
            ctx.push_log(&line);
            ctx.set_message_keep_log(&format!(
                "傳輸中… 讀 {read_spd} / 寫 {write_spd}"
            ));
            window_read_bytes = 0;
            window_write_bytes = 0;
            window_write_ms = 0;
            window_read_ms = 0;
            max_write_ms = 0;
            last_log = Instant::now();
        }
    }

    let t_flush = Instant::now();
    outfile.flush().context("最終 flush 失敗")?;
    let flush_ms = t_flush.elapsed().as_millis();
    let total_secs = transfer_start.elapsed().as_secs_f64().max(0.001);
    ctx.push_log(&format!(
        "完成 {}：{:.1} MB，用時 {:.1}s，平均 {}，最終 flush {flush_ms} ms",
        dest.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        written as f64 / (1024.0 * 1024.0),
        total_secs,
        format_mbps(written, total_secs),
    ));
    Ok(written)
}

fn download_source_file_to(
    source_host: &str,
    source_port: u16,
    remote_path: &str,
    dest: &Path,
    ctx: &ProgressCtx,
) -> anyhow::Result<u64> {
    let url = format!("http://{source_host}:{source_port}/api/v1/download");
    download_http_body_to(
        source_host,
        source_port,
        &url,
        serde_json::json!({ "path": remote_path }),
        remote_path,
        dest,
        ctx,
    )
}

fn download_xfer_source_file_to(
    source_host: &str,
    source_port: u16,
    token: &str,
    relative_path: &str,
    dest: &Path,
    ctx: &ProgressCtx,
) -> anyhow::Result<u64> {
    let url = format!("http://{source_host}:{source_port}/api/v1/xfer-source/download");
    download_http_body_to(
        source_host,
        source_port,
        &url,
        serde_json::json!({ "token": token, "path": relative_path }),
        relative_path,
        dest,
        ctx,
    )
}

fn delete_source_paths(
    client: &reqwest::blocking::Client,
    source_host: &str,
    source_port: u16,
    paths: &[String],
) -> anyhow::Result<()> {
    let url = format!("http://{source_host}:{source_port}/api/v1/file-op");
    let resp = client
        .post(&url)
        .timeout(Duration::from_secs(120))
        .json(&serde_json::json!({
            "action": "delete_permanent",
            "paths": paths,
            "destPath": "",
            "newName": "",
            "async": false,
        }))
        .send()
        .context("向來源 PC 刪除失敗")?;
    if !resp.status().is_success() {
        anyhow::bail!("來源 PC 刪除 HTTP {}", resp.status());
    }
    let body: FileOpResponseBody = resp.json().context("解析來源 PC 刪除回應失敗")?;
    if !body.ok {
        anyhow::bail!("{}", body.message);
    }
    Ok(())
}

/// 目標 PC 執行跨機貼上：從來源 PC 拉取並寫入 `dest_dir`。
pub fn execute_cross_paste(
    source_host: &str,
    source_port: u16,
    selections: &[CrossPasteSelection],
    mode: &str,
    dest_dir: &Path,
    dest_rel: &str,
    resolve_upload: &dyn Fn(&str) -> anyhow::Result<PathBuf>,
    ctx: &ProgressCtx,
) -> anyhow::Result<String> {
    if selections.is_empty() {
        anyhow::bail!("未指定來源路徑");
    }

    ctx.push_log(&format!(
        "跨 PC 貼上開始 mode={mode} 來源={source_host}:{source_port} 選取={} 項目 目標目錄={}",
        selections.len(),
        dest_dir.display()
    ));
    ctx.set_phase("scanning", "正在檢查來源 PC…");
    ctx.check_cancelled()?;

    let t_health = Instant::now();
    validate_source_reachable_with_ctx(source_host, source_port, Some(ctx))?;
    ctx.push_log(&format!(
        "來源 PC health OK（{} ms）",
        t_health.elapsed().as_millis()
    ));

    let list_client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .context("建立 HTTP 用戶端失敗")?;

    ctx.set_phase("scanning", "正在列出來源檔案…");
    ctx.push_log("開始向來源 PC 列檔…");

    let dest_base = normalize_path(dest_rel);
    let mut planned: Vec<(String, String, u64)> = Vec::new();
    let mut failed_list = 0usize;

    for (i, sel) in selections.iter().enumerate() {
        ctx.check_cancelled()?;
        ctx.push_log(&format!(
            "列檔 ({}/{}) `{}`",
            i + 1,
            selections.len(),
            sel.path
        ));
        let t_list = Instant::now();
        match list_source_files(&list_client, source_host, source_port, &sel.path) {
            Ok(files) => {
                ctx.push_log(&format!(
                    "列檔完成 `{}` → {} 個檔案（{} ms）",
                    sel.path,
                    files.len(),
                    t_list.elapsed().as_millis()
                ));
                for item in files {
                    let tail =
                        paste_dest_relative_path(&item.relative_path, &sel.path, &sel.anchor_path);
                    let dest_api = join_rel_path(&dest_base, &tail);
                    planned.push((item.relative_path, dest_api, item.size));
                }
            }
            Err(e) => {
                ctx.push_log(&format!("列檔失敗 `{}`：{e}", sel.path));
                tracing::warn!(error = %e, path = %sel.path, "cross_paste: list failed");
                failed_list += 1;
            }
        }
    }

    planned.sort_by(|a, b| a.0.cmp(&b.0));
    planned.dedup_by(|a, b| a.0 == b.0);

    if planned.is_empty() {
        if failed_list > 0 {
            anyhow::bail!("無法從來源 PC 取得任何檔案");
        }
        anyhow::bail!("沒有可貼上的檔案");
    }

    let mut top_map: HashMap<String, String> = HashMap::new();
    for (_, dest_api, _) in &planned {
        let top = top_segment(dest_api);
        if top.is_empty() || top_map.contains_key(&top) {
            continue;
        }
        let candidate = dest_dir.join(&top);
        if path_exists(&candidate) {
            let unique = unique_dest_path(dest_dir, &top);
            let new_top = unique
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(top.clone());
            top_map.insert(top, new_top);
        } else {
            top_map.insert(top.clone(), top);
        }
    }

    let mut files: Vec<PlannedFile> = Vec::new();
    for (remote, dest_api, size) in planned {
        let top = top_segment(&dest_api);
        let remapped = if let Some(new_top) = top_map.get(&top) {
            remap_top(&dest_api, &top, new_top)
        } else {
            dest_api
        };
        let dest_abs = resolve_upload(&remapped)
            .with_context(|| format!("無法解析目標路徑：{remapped}"))?;
        files.push(PlannedFile {
            remote_path: remote,
            dest_abs,
            size,
        });
    }

    let file_count = files.len() as u32;
    let bytes_total: u64 = files.iter().map(|f| f.size).sum();
    ctx.push_log(&format!(
        "準備傳輸：{} 個檔案，共 {:.2} GB → {}",
        file_count,
        bytes_total as f64 / (1024.0 * 1024.0 * 1024.0),
        dest_dir.display()
    ));
    ctx.set_bytes_total(bytes_total, "開始跨 PC 貼上…");

    let mut ok_count = 0usize;
    let mut err_count = 0usize;

    for (idx, file) in files.iter().enumerate() {
        ctx.check_cancelled()?;
        let name = file
            .dest_abs
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| file.remote_path.clone());
        ctx.set_file(idx as u32 + 1, file_count, &format!("貼上 {name}"));
        ctx.push_log(&format!(
            "開始傳第 {}/{} 個：{} ({:.1} MB)",
            idx + 1,
            file_count,
            file.remote_path,
            file.size as f64 / (1024.0 * 1024.0)
        ));

        match download_source_file_to(
            source_host,
            source_port,
            &file.remote_path,
            &file.dest_abs,
            ctx,
        ) {
            Ok(_) => ok_count += 1,
            Err(e) => {
                if ctx.cancelled() || e.to_string().contains("已取消") {
                    let _ = std::fs::remove_file(&file.dest_abs);
                    return Err(e);
                }
                ctx.push_log(&format!("檔案失敗 `{}`：{e}", file.remote_path));
                tracing::warn!(error = %e, path = %file.remote_path, "cross_paste: file failed");
                let _ = std::fs::remove_file(&file.dest_abs);
                err_count += 1;
            }
        }
    }

    if ok_count == 0 {
        anyhow::bail!("貼上失敗：無法從來源 PC 取得任何檔案");
    }

    let is_cut = mode.eq_ignore_ascii_case("cut");
    if is_cut && err_count == 0 {
        let paths: Vec<String> = selections.iter().map(|s| s.path.clone()).collect();
        if let Err(e) = delete_source_paths(&list_client, source_host, source_port, &paths) {
            tracing::warn!(error = %e, "cross_paste: cut delete source failed");
            return Ok(format!(
                "已貼上 {ok_count} 個檔案，但來源刪除失敗：{e}"
            ));
        }
    }

    if err_count > 0 {
        Ok(format!(
            "已貼上 {ok_count} 個檔案（{err_count} 個失敗）"
        ))
    } else {
        Ok(format!("已貼上 {} 項", selections.len()))
    }
}

pub fn validate_source_reachable(source_host: &str, source_port: u16) -> anyhow::Result<()> {
    validate_source_reachable_with_ctx(source_host, source_port, None)
}

pub fn validate_source_reachable_with_ctx(
    source_host: &str,
    source_port: u16,
    ctx: Option<&ProgressCtx>,
) -> anyhow::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()?;
    let url = format!("http://{source_host}:{source_port}/api/v1/health");
    // 分段重試，讓取消請求能穿插進來（單次 8s 阻塞時取消要等很久）
    let mut last_err = None;
    for attempt in 1..=4 {
        if let Some(c) = ctx {
            c.check_cancelled()?;
            if attempt == 1 {
                c.push_log(&format!("連線來源 health：{url}"));
            } else {
                c.push_log(&format!("health 重試 {attempt}/4…"));
            }
        }
        match client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                last_err = Some(anyhow!("來源 PC health HTTP {}", resp.status()));
            }
            Err(e) => {
                last_err = Some(anyhow!("無法連線來源 PC {source_host}:{source_port}：{e}"));
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(last_err.unwrap_or_else(|| anyhow!("無法連線來源 PC")))
}

#[derive(Clone, Debug)]
pub struct PullUploadFile {
    pub relative_path: String,
    pub size: u64,
}

/// 目的 PC：從來源 PC 的 xfer-source 暫存以「下載」路徑拉取並寫入分享目錄。
pub fn execute_pull_upload(
    source_host: &str,
    source_port: u16,
    token: &str,
    files: &[PullUploadFile],
    dest_rel: &str,
    on_conflict: &str,
    resolve_upload: &dyn Fn(&str, &str) -> anyhow::Result<PathBuf>,
    ctx: &ProgressCtx,
) -> anyhow::Result<String> {
    if token.trim().is_empty() {
        anyhow::bail!("缺少上傳暫存 token");
    }
    if files.is_empty() {
        anyhow::bail!("沒有可上傳的檔案");
    }

    ctx.push_log(&format!(
        "上傳(目的端拉取) 開始 來源={source_host}:{source_port} 檔案={} 目標=`{dest_rel}` conflict={on_conflict}",
        files.len()
    ));
    ctx.set_phase("scanning", "正在檢查來源 PC…");
    ctx.check_cancelled()?;

    let t_health = Instant::now();
    validate_source_reachable_with_ctx(source_host, source_port, Some(ctx))?;
    ctx.push_log(&format!(
        "來源 PC health OK（{} ms）",
        t_health.elapsed().as_millis()
    ));

    let dest_base = normalize_path(dest_rel);
    let mut planned: Vec<(String, PathBuf, u64)> = Vec::new();
    let mut skipped = 0usize;

    for f in files {
        ctx.check_cancelled()?;
        let rel = normalize_path(&f.relative_path);
        if rel.is_empty() {
            continue;
        }
        let dest_api = join_rel_path(&dest_base, &rel);
        match resolve_upload(&dest_api, on_conflict) {
            Ok(abs) => planned.push((rel, abs, f.size)),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("跳過") || on_conflict == "skip" {
                    skipped += 1;
                    ctx.push_log(&format!("跳過衝突 `{dest_api}`"));
                } else {
                    ctx.push_log(&format!("目標路徑失敗 `{dest_api}`：{e}"));
                    return Err(e);
                }
            }
        }
    }

    if planned.is_empty() {
        if skipped > 0 {
            return Ok(format!("全部跳過（{skipped} 個已存在）"));
        }
        anyhow::bail!("沒有可寫入的目標檔案");
    }

    let bytes_total: u64 = planned.iter().map(|(_, _, s)| *s).sum();
    let file_count = planned.len() as u32;
    ctx.set_bytes_total(bytes_total, "正在從來源 PC 拉取…");
    ctx.set_phase("uploading", "正在從來源 PC 拉取…");

    let mut ok_count = 0usize;
    let mut err_count = 0usize;

    for (i, (rel, dest_abs, _size)) in planned.iter().enumerate() {
        ctx.check_cancelled()?;
        let name = last_segment(rel);
        ctx.set_file(i as u32 + 1, file_count, &format!("上傳 {name}"));
        // set_file 預設 phase=pasting；上傳 UI 需要 uploading
        ctx.set_phase("uploading", &format!("上傳 {name}"));
        ctx.push_log(&format!(
            "拉取 ({}/{}) `{rel}` → {}",
            i + 1,
            planned.len(),
            dest_abs.display()
        ));

        if let Some(parent) = dest_abs.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match download_xfer_source_file_to(
            source_host,
            source_port,
            token,
            rel,
            dest_abs,
            ctx,
        ) {
            Ok(_) => ok_count += 1,
            Err(e) => {
                if ctx.cancelled() || e.to_string().contains("已取消") {
                    let _ = std::fs::remove_file(dest_abs);
                    return Err(e);
                }
                ctx.push_log(&format!("檔案失敗 `{rel}`：{e}"));
                tracing::warn!(error = %e, path = %rel, "pull_upload: file failed");
                let _ = std::fs::remove_file(dest_abs);
                err_count += 1;
            }
        }
    }

    if ok_count == 0 {
        anyhow::bail!("上傳失敗：無法從來源 PC 取得任何檔案");
    }
    if err_count > 0 {
        Ok(format!(
            "上傳完成：成功 {ok_count} 個，失敗 {err_count} 個{}",
            if skipped > 0 {
                format!("，跳過 {skipped}")
            } else {
                String::new()
            }
        ))
    } else if skipped > 0 {
        Ok(format!("上傳完成（{ok_count} 個，跳過 {skipped}）"))
    } else {
        Ok(format!("上傳完成（{ok_count} 個檔案）"))
    }
}

