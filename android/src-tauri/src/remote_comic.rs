use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

const COMIC_HTTP_TIMEOUT_SECS: u64 = 120;
pub const MIN_COMIC_REMOTE_API: u32 = 8;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComicPageItem {
    pub index: u32,
    pub caption: String,
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComicPagesResult {
    pub title: String,
    pub pages: Vec<RemoteComicPageItem>,
}

#[derive(Debug, Deserialize)]
struct ComicPagesResponse {
    ok: bool,
    title: String,
    pages: Vec<RemoteComicPageItemRaw>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteComicPageItemRaw {
    index: usize,
    caption: String,
    entry: String,
}

fn comic_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(COMIC_HTTP_TIMEOUT_SECS))
        .build()
        .context("建立 HTTP 用戶端失敗")
}

/// 向 PC 請求 ZIP 內頁面清單（需 remote_api ≥ 8）。
pub async fn fetch_remote_comic_pages(
    host: &str,
    port: u16,
    path: &str,
) -> anyhow::Result<RemoteComicPagesResult> {
    let client = comic_client()?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/comic/pages"))
        .json(&json!({ "path": path }))
        .send()
        .await
        .context("無法連線 PC")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("PC 版本不支援串流閱讀（需 remote_api ≥ {MIN_COMIC_REMOTE_API}，請更新 PC 程式）");
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let text = resp.text().await.context("讀取頁面清單失敗")?;
    let body: ComicPagesResponse = serde_json::from_str(&text).context("解析頁面清單失敗")?;
    if !body.ok {
        anyhow::bail!("PC 回應異常");
    }
    let pages = body
        .pages
        .into_iter()
        .map(|p| RemoteComicPageItem {
            index: p.index as u32,
            caption: p.caption,
            entry: p.entry,
        })
        .collect();
    Ok(RemoteComicPagesResult {
        title: body.title,
        pages,
    })
}

/// 向 PC 請求 ZIP 內單頁原始圖檔位元組（與本地閱讀相同，不轉檔）。
pub async fn fetch_remote_comic_page_image(
    host: &str,
    port: u16,
    path: &str,
    entry: &str,
) -> anyhow::Result<Vec<u8>> {
    let client = comic_client()?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/comic/page"))
        .json(&json!({ "path": path, "entry": entry }))
        .send()
        .await
        .context("無法連線 PC")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("：{body}")
            }
        );
    }
    let bytes = resp.bytes().await.context("讀取圖片失敗")?;
    Ok(bytes.to_vec())
}
