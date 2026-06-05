use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::utils;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MobileSettings {
    pub category_directory: Option<String>,
    pub download_directory: Option<String>,
    /// 外部影片播放器 .exe 路徑（留空則用系統預設關聯）
    pub external_video_player_path: Option<String>,
}

impl MobileSettings {
    fn path(app: &AppHandle) -> anyhow::Result<PathBuf> {
        Ok(crate::utils::app_data_dir_for(app)?.join("mobile_settings.json"))
    }

    pub fn load(app: &AppHandle) -> anyhow::Result<Self> {
        let path = Self::path(app)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path).context("讀取 mobile_settings.json 失敗")?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn save(&self, app: &AppHandle) -> anyhow::Result<()> {
        let path = Self::path(app)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw).context("寫入 mobile_settings.json 失敗")?;
        Ok(())
    }
}
