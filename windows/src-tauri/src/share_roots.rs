use std::path::{Path, PathBuf};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::{Config, ShareRootBinding};
use crate::volume_binding;

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

    if config.remote_management_share_roots.is_empty()
        && !config.remote_management_dirs.is_empty()
    {
        for dir in &config.remote_management_dirs {
            if dir.as_os_str().is_empty() {
                continue;
            }
            if let Ok(binding) = volume_binding::path_to_binding(dir) {
                config.remote_management_share_roots.push(binding);
            }
        }
    }

    if config.remote_management_dirs.is_empty()
        && !config.remote_management_dir.as_os_str().is_empty()
    {
        config.remote_management_dirs = vec![config.remote_management_dir.clone()];
    }
    config
        .remote_management_dirs
        .retain(|p| !p.as_os_str().is_empty());

    sync_legacy_dirs_from_bindings(config);

    if let Some(first) = config.remote_management_dirs.first() {
        config.remote_management_dir = first.clone();
    } else {
        config.remote_management_dir.clear();
    }
}

fn sync_legacy_dirs_from_bindings(config: &mut Config) {
    let bindings: Vec<ShareRootBinding> = config
        .remote_management_share_roots
        .iter()
        .filter(|b| !b.is_empty())
        .cloned()
        .collect();
    if bindings.is_empty() {
        return;
    }
    let mut dirs = Vec::new();
    for binding in &bindings {
        if let Some(path) = volume_binding::resolve_binding(binding) {
            dirs.push(path);
        }
    }
    if !dirs.is_empty() {
        config.remote_management_dirs = dirs;
    }
}

pub fn effective_share_dirs(config: &Config) -> Vec<PathBuf> {
    let bindings: Vec<&ShareRootBinding> = config
        .remote_management_share_roots
        .iter()
        .filter(|b| !b.is_empty())
        .collect();

    if !bindings.is_empty() {
        let mut dirs = Vec::new();
        for binding in bindings {
            if let Some(path) = volume_binding::resolve_binding(binding) {
                dirs.push(path);
            }
        }
        if !dirs.is_empty() {
            return dirs;
        }
    }

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

/// 分享根 UI 顯示：優先目前解析路徑，否則使用選取時的 displayHint
pub fn share_binding_display_label(binding: &ShareRootBinding) -> String {
    if let Some(path) = volume_binding::resolve_binding(binding) {
        return share_display_label(&path);
    }
    if !binding.display_hint.is_empty() {
        return binding.display_hint.clone();
    }
    if binding.relative_path.is_empty() {
        return format!("Volume {{{}}}", binding.volume_guid);
    }
    format!("Volume {{{}}} / {}", binding.volume_guid, binding.relative_path)
}

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
    let bindings: Vec<&ShareRootBinding> = config
        .remote_management_share_roots
        .iter()
        .filter(|b| !b.is_empty())
        .collect();

    let mut mounts = Vec::new();
    let mut used_labels = std::collections::HashSet::new();

    if !bindings.is_empty() {
        for binding in bindings {
            let dir = volume_binding::resolve_binding(binding).ok_or_else(|| {
                anyhow!(
                    "無法還原分享資料夾（Volume {{{}}}）：磁碟可能未連線或已重格式化",
                    binding.volume_guid
                )
            })?;
            push_share_mount(&mut mounts, &mut used_labels, Some(binding), dir)?;
        }
        return Ok(mounts);
    }

    for dir in effective_share_dirs(config) {
        push_share_mount(&mut mounts, &mut used_labels, None, dir)?;
    }
    Ok(mounts)
}

fn push_share_mount(
    mounts: &mut Vec<ShareMount>,
    used_labels: &mut std::collections::HashSet<String>,
    binding: Option<&ShareRootBinding>,
    dir: PathBuf,
) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Err(anyhow!(
            "遠端管理資料夾不存在：`{}`",
            dir.display()
        ));
    }
    let display = match binding {
        Some(b) => share_binding_display_label(b),
        None => share_display_label(&dir),
    };
    let root = std::fs::canonicalize(&dir).unwrap_or(dir);
    let label = api_label_from_display(&display, used_labels);
    mounts.push(ShareMount {
        label,
        display,
        root,
    });
    Ok(())
}

pub fn mounts_fingerprint(mounts: &[ShareMount]) -> String {
    mounts
        .iter()
        .map(|m| m.root.display().to_string())
        .collect::<Vec<_>>()
        .join("|")
}

pub fn config_mounts_fingerprint(config: &Config) -> String {
    let bindings: Vec<&ShareRootBinding> = config
        .remote_management_share_roots
        .iter()
        .filter(|b| !b.is_empty())
        .collect();
    if !bindings.is_empty() {
        return volume_binding::bindings_fingerprint(
            &bindings.iter().copied().cloned().collect::<Vec<_>>(),
        );
    }
    mounts_fingerprint(&build_share_mounts(config).unwrap_or_default())
}

pub fn share_dirs_display(config: &Config) -> String {
    let bindings: Vec<&ShareRootBinding> = config
        .remote_management_share_roots
        .iter()
        .filter(|b| !b.is_empty())
        .collect();
    if !bindings.is_empty() {
        return bindings
            .iter()
            .map(|b| share_binding_display_label(b))
            .collect::<Vec<_>>()
            .join("；");
    }
    effective_share_dirs(config)
        .iter()
        .map(|p| share_display_label(p))
        .collect::<Vec<_>>()
        .join("；")
}
