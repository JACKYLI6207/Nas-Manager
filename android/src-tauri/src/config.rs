use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::types::DownloadFormat;

const DEFAULT_API_DOMAIN: &str = "";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub cookie: String,
    pub download_dir: PathBuf,
    pub enable_file_logger: bool,
    pub download_format: DownloadFormat,
    pub proxy_mode: ProxyMode,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub comic_concurrency: usize,
    pub comic_download_interval_sec: u64,
    pub img_concurrency: usize,
    pub img_download_interval_sec: u64,
    pub download_shelf_interval_ms: u64,
    pub batch_download_interval_ms: u64,
    pub use_original_filename: bool,
    pub api_domain_mode: ApiDomainMode,
    pub custom_api_domain: String,
    /// 單次下載請求失敗後，最多再嘗試的次數（總嘗試次數 = 1 + 此值）
    pub download_retry_count: u32,
    /// 任務進入「下載失敗」時，暫停佇列並休息的秒數（0 表示關閉）
    pub download_failure_rest_sec: u64,
    /// 韓漫 TXT 收藏列表檔案路徑（可多個以 `|` 分隔；Android 可為 content:// 文件 URI）
    pub korean_txt_catalog_dir: PathBuf,
    /// 開啟韓漫下載模式時是否自動比對 TXT 列表
    pub korean_txt_duplicate_check_enabled: bool,
    /// 快照更新掃描：舊 ID 重複超過此數後才可能提早停止（預設 20）
    pub snapshot_update_duplicate_stop_count: u32,
}

impl Config {
    pub fn new(app: &AppHandle) -> anyhow::Result<Config> {
        let app_data_dir = crate::utils::app_data_dir(app)?;
        let config_path = app_data_dir.join("config.json");

        let config = if config_path.exists() {
            let config_string = std::fs::read_to_string(config_path)?;
            match serde_json::from_str(&config_string) {
                // 如果能夠直接解析為Config，則直接返回
                Ok(config) => config,
                // 否則，將預設設定與檔案中已有的設定合併
                // 以免新版本添加了新的設定項，用戶升級到新版本後，所有設定項都被重置
                Err(_) => Config::merge_config(&config_string, &app_data_dir),
            }
        } else {
            Config::default(&app_data_dir)
        };
        config.save(app)?;
        Ok(config)
    }

    pub fn save(&self, app: &AppHandle) -> anyhow::Result<()> {
        let app_data_dir = crate::utils::app_data_dir(app)?;
        let config_path = app_data_dir.join("config.json");
        let config_string = serde_json::to_string_pretty(self)?;
        std::fs::write(config_path, config_string)?;
        Ok(())
    }

    pub fn get_api_domain(&self) -> String {
        if self.api_domain_mode == ApiDomainMode::Custom {
            self.custom_api_domain.clone()
        } else {
            DEFAULT_API_DOMAIN.to_string()
        }
    }

    fn merge_config(config_string: &str, app_data_dir: &Path) -> Config {
        let Ok(mut json_value) = serde_json::from_str::<serde_json::Value>(config_string) else {
            return Config::default(app_data_dir);
        };
        let serde_json::Value::Object(ref mut map) = json_value else {
            return Config::default(app_data_dir);
        };
        let Ok(default_config_value) = serde_json::to_value(Config::default(app_data_dir)) else {
            return Config::default(app_data_dir);
        };
        let serde_json::Value::Object(default_map) = default_config_value else {
            return Config::default(app_data_dir);
        };
        for (key, value) in default_map {
            map.entry(key).or_insert(value);
        }
        let Ok(config) = serde_json::from_value(json_value) else {
            return Config::default(app_data_dir);
        };
        config
    }

    fn default(app_data_dir: &Path) -> Config {
        let download_dir = app_data_dir.join("downloads");

        Config {
            cookie: String::new(),
            download_dir,
            enable_file_logger: true,
            download_format: DownloadFormat::Server2Zip,
            proxy_mode: ProxyMode::System,
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 7890,
            comic_concurrency: 1,
            comic_download_interval_sec: 0,
            img_concurrency: 10,
            img_download_interval_sec: 1,
            download_shelf_interval_ms: 100,
            batch_download_interval_ms: 100,
            use_original_filename: false,
            api_domain_mode: ApiDomainMode::Default,
            custom_api_domain: DEFAULT_API_DOMAIN.to_string(),
            download_retry_count: 1,
            download_failure_rest_sec: 0,
            korean_txt_catalog_dir: PathBuf::new(),
            korean_txt_duplicate_check_enabled: true,
            snapshot_update_duplicate_stop_count: 20,
        }
    }
}

impl Config {
    /// 下載請求總嘗試次數（至少 1 次，至多 21 次）
    pub fn download_max_attempts(&self) -> u32 {
        self.download_retry_count.saturating_add(1).clamp(1, 21)
    }

    /// 快照更新 ID 重複停止閾值（與 PC 相同，至少 0）
    pub fn snapshot_update_duplicate_stop_limit(&self) -> i64 {
        self.snapshot_update_duplicate_stop_count.min(9999) as i64
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub enum ProxyMode {
    #[default]
    System,
    NoProxy,
    Custom,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub enum ApiDomainMode {
    #[default]
    Default,
    Custom,
}
