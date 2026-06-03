use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::pc_remote_discovery::ensure_pc_remote_api_v4;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcFileOpResult {
    pub ok: bool,
    pub message: String,
    pub clipboard_count: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOpRequest {
    action: String,
    paths: Vec<String>,
    dest_path: String,
    new_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpResponse {
    ok: bool,
    message: String,
    clipboard_count: Option<usize>,
}

pub async fn remote_pc_file_op(
    host: &str,
    port: u16,
    action: &str,
    paths: &[String],
    dest_path: &str,
    new_name: &str,
) -> anyhow::Result<RemotePcFileOpResult> {
    ensure_pc_remote_api_v4(host, port).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("建立 HTTP 用戶端失敗")?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/file-op"))
        .json(&FileOpRequest {
            action: action.to_string(),
            paths: paths.to_vec(),
            dest_path: dest_path.to_string(),
            new_name: new_name.to_string(),
        })
        .send()
        .await
        .context("PC 檔案操作請求失敗")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "PC 檔案操作 HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let body: FileOpResponse = resp.json().await.context("解析 PC 檔案操作回應失敗")?;
    if !body.ok {
        anyhow::bail!("{}", body.message);
    }
    Ok(RemotePcFileOpResult {
        ok: body.ok,
        message: body.message,
        clipboard_count: body.clipboard_count,
    })
}
