use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{bail, Context};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOpJobProgress {
    pub phase: String,
    pub file_index: u32,
    pub file_count: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub speed_bps: u64,
    pub message: String,
    pub finished: bool,
    pub error: Option<String>,
}

struct JobSlot {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<FileOpJobProgress>>,
}

static JOB: OnceLock<Mutex<Option<JobSlot>>> = OnceLock::new();

fn job_slot() -> &'static Mutex<Option<JobSlot>> {
    JOB.get_or_init(|| Mutex::new(None))
}

pub fn job_running() -> bool {
    let guard = job_slot().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .is_some_and(|j| !j.progress.lock().unwrap_or_else(|e| e.into_inner()).finished)
}

pub fn get_job_progress() -> Option<FileOpJobProgress> {
    let guard = job_slot().lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().map(|j| {
        j.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    })
}

pub fn request_job_cancel() {
    let guard = job_slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(job) = guard.as_ref() {
        job.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut p) = job.progress.lock() {
            if !p.finished {
                p.phase = "cancelling".to_string();
                p.message = "正在取消貼上…".to_string();
            }
        }
    }
}

pub struct ProgressCtx {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<FileOpJobProgress>>,
    speed_anchor: Mutex<(Instant, u64)>,
}

impl ProgressCtx {
    fn new(cancel: Arc<AtomicBool>, progress: Arc<Mutex<FileOpJobProgress>>) -> Self {
        Self {
            cancel,
            progress,
            speed_anchor: Mutex::new((Instant::now(), 0)),
        }
    }

    fn lock_progress(&self) -> std::sync::MutexGuard<'_, FileOpJobProgress> {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn check_cancelled(&self) -> anyhow::Result<()> {
        if self.cancelled() {
            bail!("已取消");
        }
        Ok(())
    }

    fn update_speed(&self, bytes_done: u64) -> u64 {
        let mut anchor = self
            .speed_anchor
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let elapsed = anchor.0.elapsed().as_secs_f64();
        if elapsed >= 0.25 {
            let delta = bytes_done.saturating_sub(anchor.1);
            let speed = (delta as f64 / elapsed) as u64;
            *anchor = (Instant::now(), bytes_done);
            speed
        } else {
            0
        }
    }

    pub fn set_phase(&self, phase: &str, message: &str) {
        let mut p = self.lock_progress();
        p.phase = phase.to_string();
        p.message = message.to_string();
        p.speed_bps = self.update_speed(p.bytes_done);
    }

    pub fn set_bytes_total(&self, bytes_total: u64, message: &str) {
        let mut p = self.lock_progress();
        p.bytes_total = bytes_total;
        p.message = message.to_string();
        p.speed_bps = self.update_speed(p.bytes_done);
    }

    pub fn set_file(&self, file_index: u32, file_count: u32, message: &str) {
        let mut p = self.lock_progress();
        p.file_index = file_index;
        p.file_count = file_count;
        p.phase = "pasting".to_string();
        p.message = message.to_string();
        p.speed_bps = self.update_speed(p.bytes_done);
    }

    pub fn add_bytes(&self, n: u64) {
        let mut p = self.lock_progress();
        p.bytes_done = p.bytes_done.saturating_add(n);
        p.speed_bps = self.update_speed(p.bytes_done);
    }
}

/// 資料夾內所有檔案大小加總（迭代，避免深層目錄 stack overflow）。
pub fn path_tree_bytes(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0u64;
    let mut dirs = VecDeque::from([path.to_path_buf()]);
    while let Some(dir) = dirs.pop_front() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let entry_path = entry.path();
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                dirs.push_back(entry_path);
            }
        }
    }
    total
}

fn copy_single_file_with_progress(
    src: &Path,
    dst: &Path,
    ctx: &ProgressCtx,
) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut infile = std::fs::File::open(src).context("開啟來源檔失敗")?;
    let mut outfile = std::fs::File::create(dst).context("建立目的檔失敗")?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        ctx.check_cancelled()?;
        let n = infile.read(&mut buf)?;
        if n == 0 {
            break;
        }
        outfile.write_all(&buf[..n])?;
        ctx.add_bytes(n as u64);
    }
    Ok(())
}

/// 迭代複製目錄樹（避免遞迴 stack overflow）。
pub fn copy_path_with_progress(src: &Path, dst: &Path, ctx: &ProgressCtx) -> anyhow::Result<()> {
    ctx.check_cancelled()?;
    if src.is_file() {
        return copy_single_file_with_progress(src, dst, ctx);
    }
    std::fs::create_dir_all(dst)?;
    let mut dirs = VecDeque::from([(src.to_path_buf(), dst.to_path_buf())]);
    while let Some((src_dir, dst_dir)) = dirs.pop_front() {
        ctx.check_cancelled()?;
        for entry in std::fs::read_dir(&src_dir)? {
            ctx.check_cancelled()?;
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst_dir.join(entry.file_name());
            if src_path.is_dir() {
                std::fs::create_dir_all(&dst_path)?;
                dirs.push_back((src_path, dst_path));
            } else {
                copy_single_file_with_progress(&src_path, &dst_path, ctx)?;
            }
        }
    }
    Ok(())
}

pub fn move_path_with_progress(
    src: &Path,
    dst: &Path,
    ctx: &ProgressCtx,
    item_bytes: u64,
) -> anyhow::Result<()> {
    ctx.check_cancelled()?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => {
            ctx.add_bytes(item_bytes);
            Ok(())
        }
        Err(_) => {
            copy_path_with_progress(src, dst, ctx)?;
            ctx.check_cancelled()?;
            remove_path_recursive(src)?;
            Ok(())
        }
    }
}

fn remove_path_recursive(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// 啟動背景貼上任務；若已有任務進行中則回傳錯誤。
pub fn start_paste_job<F>(initial: FileOpJobProgress, run: F) -> anyhow::Result<()>
where
    F: FnOnce(&ProgressCtx) -> anyhow::Result<String> + Send + 'static,
{
    let mut guard = job_slot().lock().unwrap_or_else(|e| e.into_inner());
    if guard
        .as_ref()
        .is_some_and(|j| !j.progress.lock().unwrap_or_else(|e| e.into_inner()).finished)
    {
        bail!("已有檔案操作進行中");
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new(initial));
    *guard = Some(JobSlot {
        cancel: cancel.clone(),
        progress: progress.clone(),
    });
    drop(guard);

    std::thread::Builder::new()
        .name("file-op-paste".into())
        .spawn(move || {
            let ctx = ProgressCtx::new(cancel.clone(), progress.clone());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&ctx)));
            let mut p = progress.lock().unwrap_or_else(|e| e.into_inner());
            if p.finished {
                return;
            }
            match result {
                Ok(Ok(msg)) => {
                    p.phase = "done".to_string();
                    p.message = msg;
                    p.finished = true;
                    p.error = None;
                    p.speed_bps = 0;
                }
                Ok(Err(e))
                    if cancel.load(Ordering::Relaxed) || e.to_string().contains("已取消") =>
                {
                    p.phase = "cancelled".to_string();
                    p.message = "貼上已取消".to_string();
                    p.finished = true;
                    p.error = Some("已取消".to_string());
                    p.speed_bps = 0;
                }
                Ok(Err(e)) => {
                    let msg = e.to_string();
                    p.phase = "error".to_string();
                    p.message = msg.clone();
                    p.finished = true;
                    p.error = Some(msg);
                    p.speed_bps = 0;
                }
                Err(_) => {
                    p.phase = "error".to_string();
                    p.message = "貼上時發生內部錯誤（可能資料夾過深或檔案過多）".to_string();
                    p.finished = true;
                    p.error = Some("貼上時發生內部錯誤".to_string());
                    p.speed_bps = 0;
                }
            }
        })
        .context("無法啟動貼上背景執行緒")?;

    Ok(())
}
