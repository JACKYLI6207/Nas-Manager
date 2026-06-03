use std::path::PathBuf;

use anyhow::Context;
use tauri::{AppHandle, Manager};

/// 應用私有資料目錄（Android / 桌面皆可用）
pub fn app_data_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("取得 app_data_dir 失敗")?;
    Ok(dir.join("GentlemanManager"))
}

pub fn filename_filter(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' | '/' => ' ',
            ':' => '：',
            '*' => '⭐',
            '?' => '？',
            '"' => '\'',
            '<' => '《',
            '>' => '》',
            '|' => '丨',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}
