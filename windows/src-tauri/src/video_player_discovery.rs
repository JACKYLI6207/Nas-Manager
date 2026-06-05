use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

pub const SYSTEM_DEFAULT_PLAYER_ID: &str = "__system_default__";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoPlayerOption {
    pub id: String,
    pub name: String,
    pub exe_path: Option<String>,
    pub icon_data_url: Option<String>,
}

struct CatalogEntry {
    name: &'static str,
    relative_paths: &'static [&'static str],
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        name: "VLC media player",
        relative_paths: &[
            r"VideoLAN\VLC\vlc.exe",
            r"Program Files\VideoLAN\VLC\vlc.exe",
        ],
    },
    CatalogEntry {
        name: "PotPlayer 播放專用 (64 位元)",
        relative_paths: &[
            r"DAUM\PotPlayer\PotPlayerMini64.exe",
            r"DAUM\PotPlayer\PotPlayerMini.exe",
            r"Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
            r"Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe",
        ],
    },
    CatalogEntry {
        name: "MPC-HC (x64)",
        relative_paths: &[
            r"MPC-HC\mpc-hc64.exe",
            r"Program Files\MPC-HC\mpc-hc64.exe",
            r"Program Files (x86)\K-Lite Codec Pack\MPC-HC64\mpc-hc64.exe",
            r"Program Files (x86)\MPC-HC\mpc-hc.exe",
        ],
    },
    CatalogEntry {
        name: "mpv",
        relative_paths: &[
            r"mpv\mpv.exe",
            r"Programs\mpv\mpv.exe",
            r"Program Files\mpv\mpv.exe",
        ],
    },
    CatalogEntry {
        name: "Windows Media Player",
        relative_paths: &[
            r"Windows Media Player\wmplayer.exe",
            r"Program Files\Windows Media Player\wmplayer.exe",
            r"Program Files (x86)\Windows Media Player\wmplayer.exe",
        ],
    },
    CatalogEntry {
        name: "iTunes",
        relative_paths: &[
            r"Apple Computer\iTunes\iTunes.exe",
            r"Program Files\iTunes\iTunes.exe",
            r"Program Files (x86)\iTunes\iTunes.exe",
        ],
    },
];

fn base_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA", "APPDATA"] {
        if let Ok(v) = std::env::var(key) {
            let p = PathBuf::from(v);
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    if let Ok(windir) = std::env::var("WINDIR") {
        out.push(PathBuf::from(windir));
    }
    out
}

fn normalize_exe_key(path: &str) -> String {
    dunce::canonicalize(path)
        .map(|p| p.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|_| path.replace('/', "\\").to_ascii_lowercase())
}

fn push_player(map: &mut HashMap<String, VideoPlayerOption>, name: String, exe: PathBuf) {
    if !exe.is_file() {
        return;
    }
    let path = exe.to_string_lossy().into_owned();
    let key = normalize_exe_key(&path);
    if map.contains_key(&key) {
        return;
    }
    let icon_data_url = crate::exe_icon::icon_data_url_for_path(&path);
    map.insert(
        key.clone(),
        VideoPlayerOption {
            id: key,
            name,
            exe_path: Some(path),
            icon_data_url,
        },
    );
}

fn scan_catalog(map: &mut HashMap<String, VideoPlayerOption>) {
    let bases = base_dirs();
    for entry in CATALOG {
        for rel in entry.relative_paths {
            for base in &bases {
                let candidate = if rel.starts_with("Program Files") {
                    PathBuf::from(rel)
                } else {
                    base.join(rel)
                };
                push_player(map, entry.name.to_string(), candidate);
            }
        }
    }
}

#[cfg(windows)]
fn scan_app_paths(map: &mut HashMap<String, VideoPlayerOption>) {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    const EXE_NAMES: &[&str] = &[
        "vlc.exe",
        "PotPlayerMini64.exe",
        "PotPlayerMini.exe",
        "mpc-hc64.exe",
        "mpc-hc.exe",
        "mpv.exe",
        "wmplayer.exe",
        "iTunes.exe",
    ];

    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Ok(root) = RegKey::predef(hive).open_subkey(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths",
        ) else {
            continue;
        };
        for exe in EXE_NAMES {
            let Ok(sub) = root.open_subkey(exe) else {
                continue;
            };
            let Ok(path) = sub.get_value::<String, _>("") else {
                continue;
            };
            let display = sub
                .get_value::<String, _>("FriendlyAppName")
                .unwrap_or_else(|_| friendly_name_from_exe(exe));
            push_player(map, display, PathBuf::from(path));
        }
    }
}

#[cfg(windows)]
fn friendly_name_from_exe(exe: &str) -> String {
    match exe.to_ascii_lowercase().as_str() {
        "vlc.exe" => "VLC media player".to_string(),
        "potplayermini64.exe" | "potplayermini.exe" => "PotPlayer 播放專用 (64 位元)".to_string(),
        "mpc-hc64.exe" | "mpc-hc.exe" => "MPC-HC (x64)".to_string(),
        "mpv.exe" => "mpv".to_string(),
        "wmplayer.exe" => "Windows Media Player".to_string(),
        "itunes.exe" => "iTunes".to_string(),
        _ => exe.to_string(),
    }
}

#[cfg(windows)]
fn scan_open_with_list(map: &mut HashMap<String, VideoPlayerOption>, ext: &str) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\OpenWithList"
    );
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&path) else {
        return;
    };
    for value in key.enum_values().filter_map(Result::ok) {
        let (_, reg_value) = value;
        let prog_id = reg_value.to_string();
        if prog_id.is_empty() {
            continue;
        }
        if let Some((name, exe)) = resolve_prog_id_command(&prog_id) {
            push_player(map, name, exe);
        }
    }
}

#[cfg(windows)]
fn resolve_prog_id_command(prog_id: &str) -> Option<(String, PathBuf)> {
    use winreg::enums::HKEY_CLASSES_ROOT;
    use winreg::RegKey;

    let command_key = format!(r"{prog_id}\shell\open\command");
    let Ok(key) = RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(&command_key) else {
        return None;
    };
    let Ok(raw) = key.get_value::<String, _>("") else {
        return None;
    };
    let exe = parse_command_line_exe(&raw)?;
    if !exe.is_file() {
        return None;
    }
    let name = RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(prog_id)
        .ok()
        .and_then(|k| k.get_value::<String, _>("FriendlyTypeName").ok())
        .or_else(|| {
            exe.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| prog_id.to_string());
    Some((name, exe))
}

fn parse_command_line_exe(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = if trimmed.starts_with('"') {
        trimmed.trim_start_matches('"').split('"').next()?
    } else {
        trimmed.split_whitespace().next()?
    };
    let expanded = shellexpand_env(path);
    Some(PathBuf::from(expanded))
}

fn shellexpand_env(input: &str) -> String {
    let mut out = input.to_string();
    for (key, value) in std::env::vars() {
        let token = format!("%{key}%");
        out = out.replace(&token, &value);
    }
    out
}

pub fn list_video_player_options() -> Vec<VideoPlayerOption> {
    let mut map: HashMap<String, VideoPlayerOption> = HashMap::new();
    scan_catalog(&mut map);
    #[cfg(windows)]
    {
        scan_app_paths(&mut map);
        for ext in [".mp4", ".mkv", ".avi", ".wmv"] {
            scan_open_with_list(&mut map, ext);
        }
    }

    let mut players: Vec<VideoPlayerOption> = map.into_values().collect();
    players.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));

    let mut out = vec![VideoPlayerOption {
        id: SYSTEM_DEFAULT_PLAYER_ID.to_string(),
        name: "系統預設".to_string(),
        exe_path: None,
        icon_data_url: crate::exe_icon::icon_data_url_for_system_default(),
    }];
    out.extend(players);
    out
}

pub fn player_id_from_path(path: &str) -> String {
    normalize_exe_key(path)
}

pub fn find_player_by_exe(exe_path: &str) -> Option<VideoPlayerOption> {
    let key = normalize_exe_key(exe_path);
    list_video_player_options()
        .into_iter()
        .find(|p| p.exe_path.as_deref().is_some_and(|e| normalize_exe_key(e) == key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quoted_command() {
        let p = parse_command_line_exe(r#""C:\Program Files\VLC\vlc.exe" "%1""#).unwrap();
        assert!(p.to_string_lossy().contains("vlc.exe"));
    }
}
