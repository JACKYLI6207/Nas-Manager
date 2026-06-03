use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShareMount {
    /// API 路徑第一層（URL 安全、唯一）
    pub label: String,
    /// 手機／UI 顯示用完整路徑（如 `H:\`、`E:\漫畫`）
    pub display: String,
    pub root: PathBuf,
}

fn default_share_slots() -> u32 {
    3
}

pub fn normalize_remote_share_config(config: &mut Config) {
    if config.remote_management_share_slots == 0 {
        config.remote_management_share_slots = default_share_slots();
    }
    config.remote_management_share_slots = config.remote_management_share_slots.clamp(1, 16);

    if config.remote_management_dirs.is_empty()
        && !config.remote_management_dir.as_os_str().is_empty()
    {
        config.remote_management_dirs = vec![config.remote_management_dir.clone()];
    }
    config
        .remote_management_dirs
        .retain(|p| !p.as_os_str().is_empty());

    if let Some(first) = config.remote_management_dirs.first() {
        config.remote_management_dir = first.clone();
    } else {
        config.remote_management_dir.clear();
    }
}

pub fn effective_share_dirs(config: &Config) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = if !config.remote_management_dirs.is_empty() {
        config.remote_management_dirs.clone()
    } else if !config.remote_management_dir.as_os_str().is_empty() {
        vec![config.remote_management_dir.clone()]
    } else {
        Vec::new()
    };
    if dirs.is_empty() {
        dirs.push(config.download_dir.clone());
    }
    dirs
}

pub fn share_slot_count(config: &Config) -> usize {
    config.remote_management_share_slots.clamp(1, 16) as usize
}

/// 使用者設定的路徑顯示（保留磁碟代號，如 `H:\`、`E:\漫畫`）
pub fn share_display_label(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 {
        let bytes = trimmed.as_bytes();
        if bytes.len() == 2 && bytes[1] == b':' {
            return format!("{trimmed}\\");
        }
    }
    if trimmed.is_empty() {
        raw.to_string()
    } else {
        trimmed.to_string()
    }
}

/// API 用唯一 slug（不含 `:` `\`，避免 URL／解析問題）
fn api_label_from_display(display: &str, used: &mut std::collections::HashSet<String>) -> String {
    let mut slug = display
        .replace(':', "-")
        .replace('\\', "-")
        .replace('/', "-")
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        slug = "share".to_string();
    }
    let mut label = slug.clone();
    let mut n = 2u32;
    while !used.insert(label.clone()) {
        label = format!("{slug}-{n}");
        n += 1;
    }
    label
}

pub fn build_share_mounts(config: &Config) -> anyhow::Result<Vec<ShareMount>> {
    let dirs = effective_share_dirs(config);
    let mut mounts = Vec::new();
    let mut used_labels = std::collections::HashSet::new();

    for dir in dirs {
        if !dir.is_dir() {
            return Err(anyhow!(
                "遠端管理資料夾不存在：`{}`",
                dir.display()
            ));
        }
        let display = share_display_label(&dir);
        let root = std::fs::canonicalize(&dir).unwrap_or(dir);
        let label = api_label_from_display(&display, &mut used_labels);
        mounts.push(ShareMount {
            label,
            display,
            root,
        });
    }
    Ok(mounts)
}

pub fn mounts_fingerprint(mounts: &[ShareMount]) -> String {
    mounts
        .iter()
        .map(|m| m.root.display().to_string())
        .collect::<Vec<_>>()
        .join("|")
}

pub fn share_dirs_display(config: &Config) -> String {
    effective_share_dirs(config)
        .iter()
        .map(|p| share_display_label(p))
        .collect::<Vec<_>>()
        .join("；")
}
