use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};

use crate::config::ShareRootBinding;

/// 使用者選擇的路徑 → Volume GUID + 磁區內相對路徑（不受磁碟代號變更影響）。
#[cfg(windows)]
pub fn path_to_binding(path: &Path) -> anyhow::Result<ShareRootBinding> {
    let canonical = dunce::canonicalize(path)
        .with_context(|| format!("無法解析路徑：{}", path.display()))?;

    let mount = drive_mount_point(&canonical)
        .ok_or_else(|| anyhow!("僅支援本機磁碟代號路徑（如 H:\\漫畫）；網路路徑請改用固定掛載點"))?;

    let volume_name = volume_name_for_mount(&mount)?;
    let volume_guid = parse_guid_from_volume_name(&volume_name)?;

    let mount_canon = dunce::canonicalize(&mount).unwrap_or(mount);
    let relative_path = relative_path_from_volume_root(&canonical, &mount_canon);

    Ok(ShareRootBinding {
        volume_guid,
        relative_path,
        display_hint: path.display().to_string(),
    })
}

#[cfg(not(windows))]
pub fn path_to_binding(path: &Path) -> anyhow::Result<ShareRootBinding> {
    let _ = path;
    Err(anyhow!("Volume GUID 綁定僅支援 Windows"))
}

/// 依 GUID + 相對路徑還原目前實際路徑（磁碟代號變更後仍有效）。
#[cfg(windows)]
pub fn resolve_binding(binding: &ShareRootBinding) -> Option<PathBuf> {
    if binding.volume_guid.trim().is_empty() {
        return None;
    }
    let mount = first_mount_for_guid(&binding.volume_guid)?;
    let mut path = mount;
    if !binding.relative_path.is_empty() {
        for part in binding.relative_path.split('/') {
            if part.is_empty() {
                continue;
            }
            path.push(part);
        }
    }
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn resolve_binding(_binding: &ShareRootBinding) -> Option<PathBuf> {
    None
}

pub fn bindings_fingerprint(bindings: &[ShareRootBinding]) -> String {
    bindings
        .iter()
        .filter(|b| !b.volume_guid.is_empty())
        .map(|b| format!("{}|{}", b.volume_guid, b.relative_path))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(windows)]
fn drive_mount_point(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if bytes[1] == b':' {
            let drive = format!("{}\\", &s[..2]);
            return Some(PathBuf::from(drive));
        }
    }
    None
}

#[cfg(windows)]
fn volume_name_for_mount(mount: &Path) -> anyhow::Result<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;

    let mut mount_str = mount.to_string_lossy().into_owned();
    if !mount_str.ends_with('\\') {
        mount_str.push('\\');
    }
    let mount_wide: Vec<u16> = std::ffi::OsStr::new(&mount_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut buf = vec![0u16; 256];
    unsafe {
        GetVolumeNameForVolumeMountPointW(PCWSTR(mount_wide.as_ptr()), &mut buf)
            .map_err(|e| anyhow!("GetVolumeNameForVolumeMountPointW 失敗：{e}"))?;
    }
    Ok(wide_to_string(&buf))
}

#[cfg(windows)]
fn parse_guid_from_volume_name(volume_name: &str) -> anyhow::Result<String> {
    let trimmed = volume_name.trim_end_matches('\\');
    let guid = trimmed
        .strip_prefix(r"\\?\Volume{")
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| anyhow!("無法解析 Volume GUID：{volume_name}"))?;
    Ok(guid.to_lowercase())
}

#[cfg(windows)]
fn relative_path_from_volume_root(canonical: &Path, mount_canon: &Path) -> String {
    canonical
        .strip_prefix(mount_canon)
        .map(|p| {
            let s = p.to_string_lossy();
            s.trim_start_matches(['\\', '/'])
                .replace('\\', "/")
        })
        .unwrap_or_default()
}

#[cfg(windows)]
fn volume_name_from_guid(guid: &str) -> String {
    format!(r"\\?\Volume{{{}}}\", guid.trim().to_lowercase())
}

#[cfg(windows)]
fn first_mount_for_guid(guid: &str) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetVolumePathNamesForVolumeNameW;

    let volume_name = volume_name_from_guid(guid);
    let volume_wide: Vec<u16> = std::ffi::OsStr::new(&volume_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut buf = vec![0u16; 512];
    let mut len = buf.len() as u32;
    unsafe {
        if GetVolumePathNamesForVolumeNameW(
            PCWSTR(volume_wide.as_ptr()),
            Some(&mut buf),
            &mut len,
        )
        .is_err()
        {
            return None;
        }
    }
    first_path_from_multi_sz(&buf)
}

#[cfg(windows)]
fn first_path_from_multi_sz(buf: &[u16]) -> Option<PathBuf> {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    if end == 0 {
        return None;
    }
    let s = wide_to_string(&buf[..end]);
    if s.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

#[cfg(windows)]
fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;

    #[test]
    fn parse_volume_guid() {
        let g = parse_guid_from_volume_name(r"\\?\Volume{a1b2c3d4-e5f6-7890-abcd-ef1234567890}\")
            .unwrap();
        assert_eq!(g, "a1b2c3d4-e5f6-7890-abcd-ef1234567890");
    }
}
