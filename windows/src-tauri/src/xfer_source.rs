//! 本機上傳暫存會話：目的 PC 以「下載」路徑從此處拉取檔案（僅允許已登記路徑）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use uuid::Uuid;

const SESSION_TTL: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone)]
pub struct XferSourceFile {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub size: u64,
}

struct Session {
    files: HashMap<String, XferSourceFile>,
    created: Instant,
}

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn normalize_rel(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn purge_expired(map: &mut HashMap<String, Session>) {
    let now = Instant::now();
    map.retain(|_, s| now.duration_since(s.created) < SESSION_TTL);
}

/// 登記一組本機檔案，回傳 token（目的端下載時必帶）。
pub fn register_session(files: Vec<XferSourceFile>) -> anyhow::Result<String> {
    if files.is_empty() {
        anyhow::bail!("沒有可上傳的檔案");
    }
    let mut map = HashMap::new();
    for f in files {
        let rel = normalize_rel(&f.relative_path);
        if rel.is_empty() || rel.contains("..") {
            anyhow::bail!("非法相對路徑：{}", f.relative_path);
        }
        if !f.absolute_path.is_file() {
            anyhow::bail!("來源不是檔案：{}", f.absolute_path.display());
        }
        map.insert(
            rel.clone(),
            XferSourceFile {
                relative_path: rel,
                absolute_path: f.absolute_path,
                size: f.size,
            },
        );
    }
    let token = Uuid::new_v4().to_string();
    let mut guard = sessions().lock();
    purge_expired(&mut guard);
    guard.insert(
        token.clone(),
        Session {
            files: map,
            created: Instant::now(),
        },
    );
    Ok(token)
}

pub fn unregister_session(token: &str) {
    if token.is_empty() {
        return;
    }
    sessions().lock().remove(token);
}

pub fn resolve_file(token: &str, relative_path: &str) -> anyhow::Result<PathBuf> {
    let rel = normalize_rel(relative_path);
    let mut guard = sessions().lock();
    purge_expired(&mut guard);
    let session = guard
        .get(token)
        .ok_or_else(|| anyhow::anyhow!("上傳暫存已失效或不存在"))?;
    let file = session
        .files
        .get(&rel)
        .ok_or_else(|| anyhow::anyhow!("路徑未在此次上傳暫存中：{rel}"))?;
    Ok(file.absolute_path.clone())
}
