use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use anyhow::{anyhow, Context};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use futures_util::StreamExt;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use parking_lot::Mutex as SyncMutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::{watch, Mutex as AsyncMutex},
};

use socket2::{Domain, Protocol, Socket, Type as SocketType};

use crate::{
    config::Config,
    extensions::AppHandleExt,
    local_reader,
    share_roots::{
        build_share_mounts, share_dirs_display, ShareMount,
    },
};

pub const DEFAULT_PORT: u16 = 8765;
pub const UDP_DISCOVERY_PORT: u16 = 38765;
const SERVICE_TYPE: &str = "_gentleman-manager._tcp.local.";
const DISCOVER_PACKET: &[u8] = b"GM_REMOTE_V1\nDISCOVER\n";

static SERVER_SLOT: OnceLock<SyncMutex<Option<RunningServer>>> = OnceLock::new();
static LAST_ERROR: OnceLock<SyncMutex<Option<String>>> = OnceLock::new();
static SYNC_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

struct RunningServer {
    fingerprint: String,
    stop_tx: watch::Sender<()>,
    http_task: tauri::async_runtime::JoinHandle<()>,
    udp_task: tauri::async_runtime::JoinHandle<()>,
    mdns: Option<ServiceDaemon>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteManagementStatus {
    pub enabled: bool,
    pub running: bool,
    /// 本機能否連上 HTTP health（區網連線通常亦需防火牆允許）
    pub http_reachable: bool,
    /// Windows：區網入站防火牆規則是否已就緒（8765/38765，僅私人網路＋localsubnet）
    pub firewall_ready: bool,
    pub firewall_hint: Option<String>,
    pub port: u16,
    pub display_name: String,
    pub share_dir: String,
    pub share_dirs: Vec<String>,
    pub lan_addresses: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareMountHealth {
    label: String,
    display: String,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    app: &'static str,
    version: &'static str,
    /// 2=POST path；3=上傳；4=檔案操作；5=串流；6=分享根 displayName；7=上傳不要求檔案已存在；8=漫畫串流閱讀
    remote_api: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    share_mounts: Vec<ShareMountHealth>,
}

#[derive(Clone)]
struct HttpState {
    mounts: Vec<ShareMount>,
    /// 僅一個分享根時，API 路徑與舊版相同（不強制第一層為掛載名）
    single_flat: bool,
}

impl HttpState {
    fn is_multi_root(&self) -> bool {
        !self.single_flat && self.mounts.len() > 1
    }

    fn find_mount_by_head(&self, head: &str) -> Option<&ShareMount> {
        let head = head.trim();
        if let Some(m) = self.mounts.iter().find(|m| m.label == head) {
            return Some(m);
        }
        // 相容舊版 API 根名稱（分享 / 分享@\ / 資料夾名）
        if head == "分享" || head.eq_ignore_ascii_case("share") {
            return self.mounts.first();
        }
        if head.starts_with("分享@") {
            return self.mounts.get(1);
        }
        self.mounts.iter().find(|m| {
            m.display.eq_ignore_ascii_case(head)
                || m.display.trim_end_matches(['\\', '/']) == head
                || m.label.eq_ignore_ascii_case(head)
        })
    }

    fn strip_mount_prefix(mount: &ShareMount, rel: &str) -> String {
        let rel = rel.trim().trim_start_matches(['/', '\\']);
        if rel.is_empty() {
            return String::new();
        }
        for prefix in [
            mount.label.as_str(),
            mount.display.trim_end_matches(['\\', '/']),
        ] {
            if prefix.is_empty() {
                continue;
            }
            if rel == prefix {
                return String::new();
            }
            if let Some(rest) = rel.strip_prefix(prefix) {
                let rest = rest.trim_start_matches(['/', '\\']);
                if !rest.is_empty() {
                    return rest.to_string();
                }
            }
        }
        rel.to_string()
    }

    fn resolve_mount_and_inner(&self, api_rel: &str) -> anyhow::Result<(&ShareMount, String)> {
        let rel = api_rel.trim().trim_start_matches(['/', '\\']);
        if self.single_flat || self.mounts.len() <= 1 {
            let mount = self
                .mounts
                .first()
                .ok_or_else(|| anyhow!("未設定分享資料夾"))?;
            return Ok((mount, Self::strip_mount_prefix(mount, rel)));
        }
        if rel.is_empty() {
            return Err(anyhow!("請先進入分享資料夾"));
        }
        let (head, tail) = match rel.split_once(['/', '\\']) {
            Some((h, t)) => (h.trim(), t.trim_start_matches(['/', '\\'])),
            None => (rel, ""),
        };
        let mount = self
            .find_mount_by_head(head)
            .with_context(|| format!("未知的分享根：`{head}`"))?;
        Ok((mount, tail.to_string()))
    }

    fn resolve_under_share(&self, api_rel: &str) -> anyhow::Result<PathBuf> {
        let (mount, inner) = self.resolve_mount_and_inner(api_rel)?;
        resolve_under_share(&mount.root, &inner)
    }

    /// 上傳目標：不要求檔案已存在，僅組裝分享根下的合法路徑。
    fn resolve_upload_path(&self, api_rel: &str) -> anyhow::Result<PathBuf> {
        let (mount, inner) = self.resolve_mount_and_inner(api_rel)?;
        resolve_upload_path_under_share(&mount.root, &inner)
    }

    fn resolve_browse_dir(&self, api_rel: &str) -> anyhow::Result<PathBuf> {
        let (mount, inner) = self.resolve_mount_and_inner(api_rel)?;
        resolve_browse_dir(&mount.root, &inner)
    }

    fn api_path_for(&self, mount: &ShareMount, abs: &Path) -> String {
        let inner = rel_path_from_root(&mount.root, abs);
        if self.single_flat || self.mounts.len() <= 1 {
            return inner;
        }
        if inner.is_empty() {
            mount.label.clone()
        } else {
            format!("{}/{}", mount.label, inner)
        }
    }

    fn display_path_for(&self, mount: &ShareMount, abs: &Path) -> String {
        let inner = rel_path_from_root(&mount.root, abs);
        if inner.is_empty() {
            return mount.display.clone();
        }
        let sep = if mount.display.contains('\\') { '\\' } else { '/' };
        format!(
            "{}{}{}",
            mount.display.trim_end_matches(['\\', '/']),
            sep,
            inner.replace('/', &sep.to_string())
        )
    }
}

#[derive(serde::Deserialize)]
struct BrowseQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteDirEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// 根目錄分享磁碟：可用空間（bytes）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_free_bytes: Option<u64>,
    /// 根目錄分享磁碟：總容量（bytes）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total_bytes: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowseResponse {
    ok: bool,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_display: Option<String>,
    entries: Vec<RemoteDirEntry>,
}

#[derive(serde::Deserialize)]
struct ListFilesQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteFileListItem {
    relative_path: String,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResponse {
    ok: bool,
    files: Vec<RemoteFileListItem>,
}

#[derive(serde::Deserialize)]
struct DownloadQuery {
    path: String,
}

#[derive(serde::Deserialize, Default)]
struct StreamQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    path_b64: String,
}

/// POST body：路徑放 JSON，避免 GET query 中 [ ] # 等字元被截斷。
#[derive(Deserialize)]
struct RemotePathBody {
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
struct UploadExistsBody {
    paths: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadExistsResponse {
    ok: bool,
    conflicts: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResponse {
    ok: bool,
    path: String,
}

fn set_last_error(message: Option<String>) {
    let slot = LAST_ERROR.get_or_init(|| SyncMutex::new(None));
    *slot.lock() = message;
}

pub fn clear_last_error() {
    set_last_error(None);
}

fn remote_fingerprint(config: &Config) -> String {
    let mount_fp = crate::share_roots::config_mounts_fingerprint(config);
    format!(
        "{}|{}|{}|{}",
        env!("CARGO_PKG_VERSION"),
        config.remote_management_port,
        mount_fp,
        effective_display_name(config)
    )
}

/// 服務已在跑、設定未變、且本機 HTTP 可連時，不必重啟。
pub fn is_running_with_config(config: &Config) -> bool {
    if !config.remote_management_enabled {
        return false;
    }
    let fp = remote_fingerprint(config);
    let slot_ok = SERVER_SLOT
        .get_or_init(|| SyncMutex::new(None))
        .lock()
        .as_ref()
        .is_some_and(|s| s.fingerprint == fp);
    slot_ok
}

#[cfg(windows)]
mod windows_firewall {
    use std::fs;
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    static LAST_STATUS: OnceLock<Mutex<(bool, Option<String>)>> = OnceLock::new();

    pub fn last_status() -> (bool, Option<String>) {
        LAST_STATUS
            .get_or_init(|| Mutex::new((false, None)))
            .lock()
            .map(|g| g.clone())
            .unwrap_or((false, None))
    }

    fn set_status(ok: bool, hint: Option<String>) {
        if let Ok(mut g) = LAST_STATUS
            .get_or_init(|| Mutex::new((false, None)))
            .lock()
        {
            *g = (ok, hint);
        }
    }

    fn tcp_rule_name(port: u16) -> String {
        format!("GentlemanManagerRemoteTcp{port}")
    }

    fn udp_rule_name(port: u16) -> String {
        format!("GentlemanManagerRemoteUdp{port}")
    }

    fn rule_exists(rule_name: &str) -> bool {
        Command::new("netsh")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "advfirewall",
                "firewall",
                "show",
                "rule",
                &format!("name={rule_name}"),
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn run_netsh_add(rule_name: &str, args: &[&str]) -> bool {
        let _ = Command::new("netsh")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "advfirewall",
                "firewall",
                "delete",
                "rule",
                &format!("name={rule_name}"),
            ])
            .output();
        let output = Command::new("netsh")
            .creation_flags(CREATE_NO_WINDOW)
            .args(
                [
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    &format!("name={rule_name}"),
                ]
                .into_iter()
                .chain(args.iter().copied()),
            )
            .output();
        output.map(|o| o.status.success()).unwrap_or(false)
    }

    fn add_rules_direct(tcp_port: u16, udp_port: u16, exe: Option<&Path>) -> bool {
        let tcp_ok = run_netsh_add(
            &tcp_rule_name(tcp_port),
            &[
                "dir=in",
                "action=allow",
                "protocol=TCP",
                &format!("localport={tcp_port}"),
                "remoteip=localsubnet",
                "profile=private",
                "enable=yes",
            ],
        );
        let udp_ok = run_netsh_add(
            &udp_rule_name(udp_port),
            &[
                "dir=in",
                "action=allow",
                "protocol=UDP",
                &format!("localport={udp_port}"),
                "remoteip=localsubnet",
                "profile=private",
                "enable=yes",
            ],
        );
        let mut exe_ok = true;
        if let Some(exe_path) = exe {
            let exe_str = exe_path.to_string_lossy();
            exe_ok = run_netsh_add(
                "GentlemanManagerRemoteExe",
                &[
                    "dir=in",
                    "action=allow",
                    "program",
                    &exe_str,
                    "remoteip=localsubnet",
                    "profile=private",
                    "enable=yes",
                ],
            );
        }
        tcp_ok && udp_ok && exe_ok
    }

    fn write_elevated_script(tcp_port: u16, udp_port: u16, exe: Option<&Path>) -> std::io::Result<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!(
            "gm-remote-firewall-{}-{}.ps1",
            std::process::id(),
            tcp_port
        ));
        let mut script = format!(
            r#"$ErrorActionPreference = 'SilentlyContinue'
netsh advfirewall firewall delete rule name="{tcp}" 2>$null
netsh advfirewall firewall add rule name="{tcp}" dir=in action=allow protocol=TCP localport={tcp_port} remoteip=localsubnet profile=private enable=yes
netsh advfirewall firewall delete rule name="{udp}" 2>$null
netsh advfirewall firewall add rule name="{udp}" dir=in action=allow protocol=UDP localport={udp_port} remoteip=localsubnet profile=private enable=yes
"#,
            tcp = tcp_rule_name(tcp_port),
            udp = udp_rule_name(udp_port),
            tcp_port = tcp_port,
            udp_port = udp_port,
        );
        if let Some(exe_path) = exe {
            let exe_escaped = exe_path.to_string_lossy().replace('\'', "''");
            script.push_str(&format!(
                r#"netsh advfirewall firewall delete rule name="GentlemanManagerRemoteExe" 2>$null
netsh advfirewall firewall add rule name="GentlemanManagerRemoteExe" dir=in action=allow program='{exe_escaped}' remoteip=localsubnet profile=private enable=yes
"#
            ));
        }
        let mut file = fs::File::create(&path)?;
        file.write_all(script.as_bytes())?;
        Ok(path)
    }

    fn run_elevated_script(script_path: &Path) -> bool {
        let path_str = script_path.to_string_lossy();
        let arg_file = format!("-NoProfile -ExecutionPolicy Bypass -File \"{path_str}\"");
        let ps = format!(
            "Start-Process -FilePath powershell.exe -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '{arg_file}'"
        );
        Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn rules_verified(tcp_port: u16, _udp_port: u16) -> bool {
        rule_exists(&tcp_rule_name(tcp_port)) || rule_exists("GentlemanManagerRemoteExe")
    }

    pub fn is_ready(tcp_port: u16, udp_port: u16) -> bool {
        rules_verified(tcp_port, udp_port)
    }

    pub fn ensure_rules(tcp_port: u16, udp_port: u16) {
        let exe = std::env::current_exe().ok();
        let exe_ref = exe.as_deref();

        if rules_verified(tcp_port, udp_port) {
            set_status(true, None);
            return;
        }

        let _ = add_rules_direct(tcp_port, udp_port, exe_ref);
        if rules_verified(tcp_port, udp_port) {
            set_status(true, None);
            return;
        }

        if let Ok(script_path) = write_elevated_script(tcp_port, udp_port, exe_ref) {
            let _ = run_elevated_script(&script_path);
            let _ = fs::remove_file(&script_path);
        }

        if rules_verified(tcp_port, udp_port) {
            set_status(true, None);
            return;
        }

        set_status(
            false,
            Some(
                "區網防火牆規則尚未寫入。請將 Windows 網路設為「私人」，啟動遠端管理時在 UAC 視窗按「是」；私人防火牆請保持開啟，無須整體關閉。"
                    .to_string(),
            ),
        );
    }
}

#[cfg(windows)]
fn ensure_firewall_rules(tcp_port: u16, udp_port: u16) {
    windows_firewall::ensure_rules(tcp_port, udp_port);
}

#[cfg(not(windows))]
fn ensure_firewall_rules(_tcp_port: u16, _udp_port: u16) {}

async fn bind_tcp_listener(port: u16) -> anyhow::Result<TcpListener> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    if let Ok(listener) = TcpListener::bind(addr).await {
        return Ok(listener);
    }
    let socket = Socket::new(Domain::IPV4, SocketType::STREAM, Some(Protocol::TCP))
        .context("建立 TCP socket 失敗")?;
    socket.set_reuse_address(true).context("設定 SO_REUSEADDR 失敗")?;
    socket.bind(&addr.into()).with_context(|| {
        format!(
            "綁定 TCP port {port} 失敗：可能已有舊版 Nas-Manager-Windows.exe 占用。\
             請在工作管理員結束所有 Nas-Manager-Windows.exe 後再開啟新版，\
             或在 PowerShell 執行：Get-NetTCPConnection -LocalPort {port} | Select OwningProcess"
        )
    })?;
    socket.set_nonblocking(true).context("設定非阻塞模式失敗")?;
    TcpListener::from_std(socket.into()).context("轉換 TCP listener 失敗")
}

async fn stop_async() {
    let server = SERVER_SLOT.get_or_init(|| SyncMutex::new(None)).lock().take();
    let Some(server) = server else {
        return;
    };
    let _ = server.stop_tx.send(());
    server.http_task.abort();
    server.udp_task.abort();
    if let Some(mdns) = server.mdns {
        let _ = mdns.shutdown();
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
}

/// 強制重啟遠端管理（設定頁「重新啟動」用，略過「已在跑」快取）。
pub async fn force_start_async(app: &AppHandle) {
    let lock = SYNC_LOCK.get_or_init(|| AsyncMutex::new(()));
    let _guard = lock.lock().await;
    let config = app.get_config().read().clone();
    if !config.remote_management_enabled {
        set_last_error(None);
        stop_async().await;
        return;
    }
    set_last_error(None);
    stop_async().await;
    if let Err(err) = start_async(app, &config).await {
        let msg = err.to_string();
        tracing::error!(message = %msg, "啟動遠端管理服務失敗");
        set_last_error(Some(msg));
        stop_async().await;
    }
}

/// 依設定啟動或停止區網遠端管理（非阻塞；實際工作在 async runtime）。
pub fn sync(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        sync_async(&handle).await;
    });
}

/// 同步等待遠端管理啟停完成（儲存設定時使用）。
pub async fn sync_async(app: &AppHandle) {
    let lock = SYNC_LOCK.get_or_init(|| AsyncMutex::new(()));
    let _guard = lock.lock().await;

    let config = app.get_config().read().clone();
    if config.remote_management_enabled {
        if is_running_with_config(&config) {
            set_last_error(None);
            return;
        }
        set_last_error(None);
        if let Err(err) = start_async(app, &config).await {
            let msg = err.to_string();
            tracing::error!(message = %msg, "啟動遠端管理服務失敗");
            set_last_error(Some(msg));
            stop_async().await;
        }
    } else {
        set_last_error(None);
        stop_async().await;
    }
}

pub fn stop() {
    tauri::async_runtime::spawn(async {
        stop_async().await;
    });
}

async fn start_async(app: &AppHandle, config: &Config) -> anyhow::Result<()> {
    let mounts = build_share_mounts(config)?;
    if mounts.is_empty() {
        return Err(anyhow!("請至少指定一個分享資料夾"));
    }
    let single_flat = mounts.len() == 1;

    let port = config.remote_management_port;
    let display_name = effective_display_name(config);
    let fingerprint = remote_fingerprint(config);
    let lan_ips = list_lan_ipv4_addresses();
    let lan_ip = lan_ips
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1".to_string());

    stop_async().await;

    let listener = bind_tcp_listener(port).await?;
    ensure_firewall_rules(port, UDP_DISCOVERY_PORT);

    let (stop_tx, stop_rx) = watch::channel(());

    let share_count = mounts.len();
    let http_state = HttpState {
        mounts,
        single_flat,
    };
    let mut http_stop = stop_rx.clone();
    let http_task = tauri::async_runtime::spawn(async move {
        let app = Router::new()
            .route("/api/v1/health", get(health_handler))
            .route("/api/v1/browse", get(browse_handler).post(browse_post_handler))
            .route(
                "/api/v1/list-files",
                get(list_files_handler).post(list_files_post_handler),
            )
            .route("/api/v1/download", get(download_handler).post(download_post_handler))
            .route("/api/v1/stream", get(stream_handler).post(stream_post_handler))
            .route(
                "/api/v1/comic/pages",
                post(comic_pages_post_handler),
            )
            .route("/api/v1/comic/page", post(comic_page_post_handler))
            .route("/api/v1/upload-exists", post(upload_exists_post_handler))
            .route("/api/v1/upload", post(upload_post_handler))
            .route("/api/v1/file-op", post(file_op_post_handler))
            .with_state(http_state);
        if let Err(err) = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = http_stop.changed().await;
            })
            .await
        {
            tracing::error!(message = %err, "遠端管理 HTTP 服務結束");
        }
    });

    let udp_stop = stop_rx.clone();
    let udp_name = display_name.clone();
    let udp_ips = lan_ips.clone();
    let udp_task = tauri::async_runtime::spawn(async move {
        if let Err(err) = run_udp_discovery(&udp_name, port, &udp_ips, udp_stop).await {
            tracing::warn!(message = %err, "UDP 探索服務未啟動（不影響 HTTP 連線）");
        }
    });

    let mdns = match ServiceDaemon::new() {
        Ok(daemon) => {
            let host_name = format!("{display_name}.local.");
            let properties = [("version", env!("CARGO_PKG_VERSION"))];
            match ServiceInfo::new(
                SERVICE_TYPE,
                &display_name,
                &host_name,
                &lan_ip,
                port,
                &properties[..],
            ) {
                Ok(service_info) => match daemon.register(service_info) {
                    Ok(()) => Some(daemon),
                    Err(err) => {
                        tracing::warn!(message = %err, "註冊 mDNS 失敗（仍可用 UDP 探索）");
                        None
                    }
                },
                Err(err) => {
                    tracing::warn!(message = %err, "建立 mDNS 服務資訊失敗");
                    None
                }
            }
        }
        Err(err) => {
            tracing::warn!(message = %err, "建立 mDNS 服務失敗（仍可用 UDP 探索）");
            None
        }
    };

    let slot = SERVER_SLOT.get_or_init(|| SyncMutex::new(None));
    *slot.lock() = Some(RunningServer {
        fingerprint,
        stop_tx,
        http_task,
        udp_task,
        mdns,
    });

    tracing::info!(
        port,
        ip = %lan_ip,
        shares = share_count,
        "遠端管理服務已啟動"
    );
    let _ = app;
    Ok(())
}

async fn run_udp_discovery(
    display_name: &str,
    http_port: u16,
    lan_ips: &[String],
    mut stop_rx: watch::Receiver<()>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", UDP_DISCOVERY_PORT))
        .await
        .context("綁定 UDP 探索埠失敗")?;
    socket
        .set_broadcast(true)
        .context("啟用 UDP broadcast 失敗")?;

    let mut buf = [0_u8; 512];
    loop {
        tokio::select! {
            _ = stop_rx.changed() => break,
            recv = socket.recv_from(&mut buf) => {
                let Ok((len, peer)) = recv else { continue };
                if len < DISCOVER_PACKET.len() || &buf[..DISCOVER_PACKET.len()] != DISCOVER_PACKET {
                    continue;
                }
                let mut reply = format!("GM_REMOTE_V1\nOK\n{display_name}\n{http_port}\n");
                for ip in lan_ips {
                    reply.push_str(ip);
                    reply.push('\n');
                }
                let _ = socket.send_to(reply.as_bytes(), peer).await;
            }
        }
    }
    Ok(())
}

async fn health_handler(State(state): State<HttpState>) -> Json<HealthResponse> {
    let share_mounts = state
        .mounts
        .iter()
        .map(|m| ShareMountHealth {
            label: m.label.clone(),
            display: m.display.clone(),
        })
        .collect();
    Json(HealthResponse {
        ok: true,
        app: "Nas Manager",
        version: env!("CARGO_PKG_VERSION"),
        remote_api: 9,
        share_mounts,
    })
}

fn resolve_under_share(share_root: &PathBuf, rel_path: &str) -> anyhow::Result<PathBuf> {
    let rel = rel_path.trim().trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return canonicalize_path(share_root);
    }
    let root = canonicalize_path(share_root)
        .with_context(|| format!("分享根目錄無效：{}", share_root.display()))?;
    let target = assemble_under_share(&root, rel)?;
    if !path_exists(&target) {
        return Err(anyhow!(
            "路徑不存在或無法存取（路徑過長請在 Windows 啟用長路徑）：`{rel}`"
        ));
    }
    canonicalize_path(&target)
        .with_context(|| format!("無法解析路徑：`{rel}`"))
}

/// 上傳寫入路徑：檔案尚不存在時仍合法（僅驗證在分享根下組裝）。
fn resolve_upload_path_under_share(share_root: &Path, rel_path: &str) -> anyhow::Result<PathBuf> {
    let rel = rel_path.trim().trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return Err(anyhow!("上傳路徑不可為空"));
    }
    let root = dunce::canonicalize(share_root).unwrap_or_else(|_| share_root.to_path_buf());
    assemble_under_share(&root, rel)
}

/// 僅用 `/`、`\` 分段組裝，避免 `Path::components` 對特殊檔名誤判。
fn assemble_under_share(root: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    let mut target = root.to_path_buf();
    for part in rel.split(['/', '\\']) {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(anyhow!("路徑不合法（不可含 ..）"));
        }
        target.push(part);
    }
    Ok(target)
}

fn path_exists(path: &Path) -> bool {
    if path.exists() {
        return true;
    }
    #[cfg(windows)]
    {
        return to_extended_path(path).exists();
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn to_extended_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    if s.starts_with(r"\\") {
        return PathBuf::from(format!(r"\\?\UNC{}", &s[1..]));
    }
    PathBuf::from(format!(r"\\?\{}", s))
}

fn canonicalize_path(path: &Path) -> anyhow::Result<PathBuf> {
    if let Ok(p) = dunce::canonicalize(path) {
        return Ok(p);
    }
    #[cfg(windows)]
    {
        let ext = to_extended_path(path);
        if let Ok(p) = dunce::canonicalize(&ext) {
            return Ok(p);
        }
        if ext.exists() {
            return Ok(ext);
        }
    }
    Err(anyhow!(
        "無法解析路徑 `{}`（若路徑很長請在 Windows 啟用長路徑支援）",
        path.display()
    ))
}

fn resolve_browse_dir(share_root: &PathBuf, rel_path: &str) -> anyhow::Result<PathBuf> {
    let target = resolve_under_share(share_root, rel_path)?;
    if !target.is_dir() {
        return Err(anyhow!("不是資料夾"));
    }
    Ok(target)
}

fn compare_entry_name(a: &str, b: &str) -> std::cmp::Ordering {
    natord::compare(a, b)
}

fn rel_path_from_root(share_root: &Path, item: &Path) -> String {
    let root = dunce::canonicalize(share_root).unwrap_or_else(|_| share_root.to_path_buf());
    let item = dunce::canonicalize(item).unwrap_or_else(|_| item.to_path_buf());
    if let Ok(rest) = item.strip_prefix(&root) {
        return rest.to_string_lossy().replace('\\', "/");
    }
    #[cfg(windows)]
    {
        fn norm(p: &Path) -> String {
            p.to_string_lossy().replace('/', "\\").to_lowercase()
        }
        let i = norm(&item);
        let r = norm(&root);
        if i == r {
            return String::new();
        }
        let prefix = format!("{r}\\");
        if i.starts_with(&prefix) {
            return i[prefix.len()..].replace('\\', "/");
        }
    }
    String::new()
}

fn list_directory_entries(dir: &Path) -> anyhow::Result<Vec<RemoteDirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).context("讀取資料夾失敗")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        let meta = entry.metadata()?;
        let child_path = entry.path();
        let size = if meta.is_file() {
            Some(meta.len())
        } else if meta.is_dir() {
            Some(directory_contents_size(&child_path))
        } else {
            None
        };
        entries.push(RemoteDirEntry {
            name,
            display_name: None,
            is_dir: meta.is_dir(),
            size,
            disk_free_bytes: None,
            disk_total_bytes: None,
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => compare_entry_name(&a.name, &b.name),
    });
    Ok(entries)
}

/// 資料夾內所有檔案大小加總（含子資料夾，不含資料夾本身 metadata）。
fn directory_contents_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_file() {
            total += meta.len();
        } else if meta.is_dir() {
            total += directory_contents_size(&path);
        }
    }
    total
}

#[cfg(windows)]
fn query_disk_space(path: &Path) -> (Option<u64>, Option<u64>) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_avail = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    unsafe {
        if GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_avail),
            Some(&mut total),
            Some(&mut total_free),
        )
        .is_ok()
        {
            return (Some(free_avail), Some(total));
        }
    }
    (None, None)
}

#[cfg(not(windows))]
fn query_disk_space(_path: &Path) -> (Option<u64>, Option<u64>) {
    (None, None)
}

fn collect_files_under(
    share_root: &Path,
    path: &Path,
    rel_prefix: &str,
    out: &mut Vec<RemoteFileListItem>,
) -> anyhow::Result<()> {
    if path.is_file() {
        let rel = if rel_prefix.is_empty() {
            rel_path_from_root(share_root, path)
        } else {
            rel_prefix.replace('\\', "/")
        };
        let size = path.metadata()?.len();
        out.push(RemoteFileListItem {
            relative_path: rel,
            size,
        });
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path).context("讀取資料夾失敗")? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        let child = entry.path();
        let rel = if rel_prefix.is_empty() {
            name
        } else {
            format!("{rel_prefix}/{name}")
        };
        collect_files_under(share_root, &child, &rel, out)?;
    }
    Ok(())
}

async fn browse_handler(
    State(state): State<HttpState>,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<BrowseResponse>, (StatusCode, String)> {
    serve_browse(&state, &query.path).await
}

async fn browse_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<RemotePathBody>,
) -> Result<Json<BrowseResponse>, (StatusCode, String)> {
    serve_browse(&state, &body.path).await
}

async fn serve_browse(
    state: &HttpState,
    rel_path: &str,
) -> Result<Json<BrowseResponse>, (StatusCode, String)> {
    let rel = rel_path.trim().trim_start_matches(['/', '\\']);
    if state.is_multi_root() && rel.is_empty() {
        let mut entries: Vec<RemoteDirEntry> = state
            .mounts
            .iter()
            .map(|m| {
                let (disk_free_bytes, disk_total_bytes) = query_disk_space(&m.root);
                RemoteDirEntry {
                    name: m.label.clone(),
                    display_name: Some(m.display.clone()),
                    is_dir: true,
                    size: None,
                    disk_free_bytes,
                    disk_total_bytes,
                }
            })
            .collect();
        entries.sort_by(|a, b| compare_entry_name(&a.name, &b.name));
        return Ok(Json(BrowseResponse {
            ok: true,
            path: String::new(),
            path_display: None,
            entries,
        }));
    }
    match state.resolve_browse_dir(rel_path) {
        Ok(dir) => {
            let (mount, _) = state
                .resolve_mount_and_inner(rel_path)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            let api_path = state.api_path_for(mount, &dir);
            let path_display = state.display_path_for(mount, &dir);
            let entries = tokio::task::spawn_blocking(move || list_directory_entries(&dir))
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(Json(BrowseResponse {
                ok: true,
                path: api_path,
                path_display: Some(path_display),
                entries,
            }))
        }
        Err(err) => Err((StatusCode::BAD_REQUEST, err.to_string())),
    }
}

async fn list_files_handler(
    State(state): State<HttpState>,
    Query(query): Query<ListFilesQuery>,
) -> Result<Json<ListFilesResponse>, (StatusCode, String)> {
    serve_list_files(&state, &query.path).await
}

async fn list_files_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<RemotePathBody>,
) -> Result<Json<ListFilesResponse>, (StatusCode, String)> {
    serve_list_files(&state, &body.path).await
}

async fn serve_list_files(
    state: &HttpState,
    rel_path: &str,
) -> Result<Json<ListFilesResponse>, (StatusCode, String)> {
    match state.resolve_under_share(rel_path) {
        Ok(path) => {
            let (mount, _) = state
                .resolve_mount_and_inner(rel_path)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
            let api_prefix = state.api_path_for(mount, &path);
            if path.is_file() {
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                return Ok(Json(ListFilesResponse {
                    ok: true,
                    files: vec![RemoteFileListItem {
                        relative_path: api_prefix,
                        size,
                    }],
                }));
            }
            let mut files = Vec::new();
            match collect_files_under(&mount.root, &path, &api_prefix, &mut files) {
                Ok(()) => {
                    files.sort_by(|a, b| compare_entry_name(&a.relative_path, &b.relative_path));
                    Ok(Json(ListFilesResponse { ok: true, files }))
                }
                Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
            }
        }
        Err(err) => Err((StatusCode::BAD_REQUEST, err.to_string())),
    }
}

async fn download_handler(
    State(state): State<HttpState>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, String)> {
    serve_download(&state, &query.path).await
}

async fn download_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<RemotePathBody>,
) -> Result<Response, (StatusCode, String)> {
    serve_download(&state, &body.path).await
}

async fn stream_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(query): Query<StreamQuery>,
) -> Result<Response, (StatusCode, String)> {
    let rel = resolve_stream_rel_path(&query, None)
        .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    serve_stream(&state, &rel, &headers).await
}

async fn stream_post_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<RemotePathBody>,
) -> Result<Response, (StatusCode, String)> {
    let rel = resolve_stream_rel_path(&StreamQuery::default(), Some(&body.path))
        .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    serve_stream(&state, &rel, &headers).await
}

fn resolve_stream_rel_path(query: &StreamQuery, body_path: Option<&str>) -> Result<String, String> {
    if let Some(p) = body_path.filter(|s| !s.trim().is_empty()) {
        return Ok(p.trim().to_string());
    }
    if !query.path_b64.trim().is_empty() {
        let b64 = query.path_b64.trim();
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(b64))
            .map_err(|_| "path_b64 解碼失敗".to_string())?;
        return String::from_utf8(bytes).map_err(|_| "path_b64 非 UTF-8".to_string());
    }
    if !query.path.trim().is_empty() {
        return Ok(query.path.trim().to_string());
    }
    Err("缺少 path".to_string())
}

fn video_mime_from_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "ts" | "m2ts" => "video/mp2t",
        "flv" => "video/x-flv",
        "wmv" => "video/x-ms-wmv",
        "rmvb" | "rm" => "application/vnd.rn-realmedia-vbr",
        _ => "application/octet-stream",
    }
}

fn image_mime_from_entry(entry: &str) -> &'static str {
    let ext = entry
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "image/jpeg",
    }
}

#[derive(Deserialize)]
struct ComicPageRequestBody {
    path: String,
    entry: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComicPageItem {
    index: usize,
    caption: String,
    entry: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComicPagesResponse {
    ok: bool,
    title: String,
    pages: Vec<ComicPageItem>,
}

async fn comic_pages_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<RemotePathBody>,
) -> Result<Json<ComicPagesResponse>, (StatusCode, String)> {
    serve_comic_pages(&state, &body.path).await
}

async fn serve_comic_pages(
    state: &HttpState,
    rel_path: &str,
) -> Result<Json<ComicPagesResponse>, (StatusCode, String)> {
    let path = state
        .resolve_under_share(rel_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "不是檔案".to_string()));
    }
    let loaded = local_reader::load_local_reader_pages(
        path.to_str()
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "路徑編碼無效".to_string()))?,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut pages = Vec::with_capacity(loaded.pages.len());
    for (index, page) in loaded.pages.into_iter().enumerate() {
        let entry = local_reader::entry_from_page_id(&page.page_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        pages.push(ComicPageItem {
            index,
            caption: page.caption,
            entry,
        });
    }
    Ok(Json(ComicPagesResponse {
        ok: true,
        title: loaded.title,
        pages,
    }))
}

async fn comic_page_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<ComicPageRequestBody>,
) -> Result<Response, (StatusCode, String)> {
    serve_comic_page(&state, &body.path, &body.entry).await
}

async fn serve_comic_page(
    state: &HttpState,
    rel_path: &str,
    entry: &str,
) -> Result<Response, (StatusCode, String)> {
    let entry = entry.trim();
    if entry.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "entry 不可為空".to_string()));
    }
    let path = state
        .resolve_under_share(rel_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "不是檔案".to_string()));
    }
    let bytes = local_reader::read_comic_zip_page(&path, entry)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let mime = image_mime_from_entry(entry);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .body(Body::from(bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// 解析 `Range: bytes=start-end`（支援 `start-` 與尾端 `-suffix`）。
fn parse_byte_range(header_value: &str, file_size: u64) -> Option<(u64, u64)> {
    if file_size == 0 {
        return None;
    }
    let spec = header_value.trim().strip_prefix("bytes=")?;
    let (start_s, end_s) = spec.split_once('-')?;
    let (start, end) = if start_s.is_empty() {
        let suffix: u64 = end_s.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let suffix = suffix.min(file_size);
        (file_size - suffix, file_size - 1)
    } else {
        let start: u64 = start_s.parse().ok()?;
        let end = if end_s.is_empty() {
            file_size - 1
        } else {
            end_s.parse().ok()?
        };
        (start, end)
    };
    if start >= file_size || start > end {
        return None;
    }
    Some((start, end.min(file_size - 1)))
}

async fn serve_stream(
    state: &HttpState,
    rel_path: &str,
    headers: &HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let path = match state.resolve_under_share(rel_path) {
        Ok(p) => p,
        Err(err) => return Err((StatusCode::BAD_REQUEST, err.to_string())),
    };
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "不是檔案".to_string()));
    }
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let file_size = meta.len();
    let mime = video_mime_from_path(&path);

    if let Some(range_header) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_byte_range(range_header, file_size) {
            let len = end - start + 1;
            let mut file = File::open(&path)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            file.seek(std::io::SeekFrom::Start(start))
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let body = Body::from_stream(ReaderStream::new(file.take(len as u64)));
            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{file_size}"),
                )
                .header(header::CONTENT_LENGTH, len.to_string())
                .body(body)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
        }
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
            .body(Body::empty())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    let file = File::open(&path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let body = Body::from_stream(ReaderStream::new(file));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, file_size.to_string())
        .body(body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn serve_download(
    state: &HttpState,
    rel_path: &str,
) -> Result<Response, (StatusCode, String)> {
    let path = match state.resolve_under_share(rel_path) {
        Ok(p) => p,
        Err(err) => return Err((StatusCode::BAD_REQUEST, err.to_string())),
    };
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "不是檔案".to_string()));
    }
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    };
    let len = file.metadata().await.ok().map(|m| m.len());
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let mut builder = Response::builder().header("content-type", "application/octet-stream");
    if let Some(len) = len {
        builder = builder.header("content-length", len.to_string());
    }
    builder
        .body(body)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn join_rel_path(base: &str, tail: &str) -> String {
    let base = base.trim().trim_matches(['/', '\\']);
    let tail = tail.trim().trim_matches(['/', '\\']);
    if base.is_empty() {
        tail.to_string()
    } else if tail.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{tail}")
    }
}

fn unique_keep_both_path(target: &Path) -> PathBuf {
    if !path_exists(target) {
        return target.to_path_buf();
    }
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let file_name = target
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (file_name.clone(), String::new()),
    };
    for n in 1..=9999 {
        let candidate = parent.join(format!("{stem} ({n}){ext}"));
        if !path_exists(&candidate) {
            return candidate;
        }
    }
    parent.join(format!("{stem} ({}){ext}", uuid::Uuid::new_v4().simple()))
}

fn api_path_from_absolute(state: &HttpState, absolute: &Path) -> Option<String> {
    for mount in &state.mounts {
        if let Some(inner) = rel_path_from_share(&mount.root, absolute) {
            if state.single_flat || state.mounts.len() <= 1 {
                return Some(inner);
            }
            if inner.is_empty() {
                return Some(mount.label.clone());
            }
            return Some(format!("{}/{}", mount.label, inner));
        }
    }
    None
}

fn resolve_upload_target_state(
    state: &HttpState,
    rel_path: &str,
    on_conflict: &str,
) -> Result<PathBuf, String> {
    let rel = rel_path.trim();
    if rel.is_empty() {
        return Err("路徑不可為空".to_string());
    }
    // 上傳新檔：只組裝路徑，不可走 resolve_under_share（該函式要求路徑已存在）
    let target = state
        .resolve_upload_path(rel)
        .map_err(|e| format!("上傳路徑無效：{e}"))?;
    if !path_exists(&target) {
        return Ok(target);
    }
    if !target.is_file() {
        return Err("目標已存在且不是檔案".to_string());
    }
    match on_conflict {
        "overwrite" => Ok(target),
        "keep_both" => Ok(unique_keep_both_path(&target)),
        other => Err(format!("不支援的衝突處理：{other}")),
    }
}

async fn upload_exists_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<UploadExistsBody>,
) -> Result<Json<UploadExistsResponse>, (StatusCode, String)> {
    let mut conflicts = Vec::new();
    for rel in body.paths {
        let rel = rel.trim();
        if rel.is_empty() {
            continue;
        }
        match state.resolve_upload_path(rel) {
            Ok(path) => {
                if path.is_dir() {
                    continue;
                }
                if path.is_file() || path_exists(&path) {
                    conflicts.push(rel.to_string());
                }
            }
            Err(_) => {}
        }
    }
    conflicts.sort();
    conflicts.dedup();
    Ok(Json(UploadExistsResponse {
        ok: true,
        conflicts,
    }))
}

fn read_upload_rel_path(headers: &HeaderMap) -> Result<String, (StatusCode, String)> {
    if let Some(b64) = header_value(headers, "x-gm-rel-path-b64") {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|_| (StatusCode::BAD_REQUEST, "路徑解碼失敗".to_string()))?;
        return String::from_utf8(bytes)
            .map_err(|_| (StatusCode::BAD_REQUEST, "路徑非 UTF-8".to_string()));
    }
    header_value(headers, "x-gm-rel-path").ok_or((
        StatusCode::BAD_REQUEST,
        "缺少 X-GM-Rel-Path（或 X-GM-Rel-Path-B64）".to_string(),
    ))
}

async fn upload_post_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let rel_path = read_upload_rel_path(&headers)?;
    let on_conflict = header_value(&headers, "x-gm-on-conflict").unwrap_or_else(|| "overwrite".to_string());
    let target = resolve_upload_target_state(&state, &rel_path, &on_conflict)
        .map_err(|msg| (StatusCode::BAD_REQUEST, msg))?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    let mut file = tokio::fs::File::create(&target)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    file.flush()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let written = api_path_from_absolute(&state, &target).unwrap_or(rel_path);
    Ok(Json(UploadResponse {
        ok: true,
        path: written,
    }))
}

fn rel_path_from_share(share_root: &Path, absolute: &Path) -> Option<String> {
    let root = dunce::canonicalize(share_root).ok()?;
    let abs = dunce::canonicalize(absolute).ok()?;
    let root_s = root.to_string_lossy();
    let abs_s = abs.to_string_lossy();
    if abs_s.starts_with(&*root_s) {
        let mut rest = abs_s[root_s.len()..].replace('\\', "/");
        if rest.starts_with('/') {
            rest = rest[1..].to_string();
        }
        return Some(rest);
    }
    None
}

pub fn get_status(config: &Config) -> RemoteManagementStatus {
    let enabled = config.remote_management_enabled;
    let running = is_running_with_config(config);
    let http_reachable = running;
    let last_error = LAST_ERROR
        .get_or_init(|| SyncMutex::new(None))
        .lock()
        .clone();

    #[cfg(windows)]
    let (firewall_ready, firewall_hint) = if enabled {
        if windows_firewall::is_ready(config.remote_management_port, UDP_DISCOVERY_PORT) {
            (true, None)
        } else {
            windows_firewall::last_status()
        }
    } else {
        (true, None)
    };
    #[cfg(not(windows))]
    let (firewall_ready, firewall_hint) = (true, None);

    RemoteManagementStatus {
        enabled,
        running,
        http_reachable,
        firewall_ready,
        firewall_hint,
        port: config.remote_management_port,
        display_name: effective_display_name(config),
        share_dir: share_dirs_display(config),
        share_dirs: crate::share_roots::effective_share_dirs(config)
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        lan_addresses: list_lan_ipv4_addresses(),
        last_error,
    }
}

pub fn effective_share_dir(config: &Config) -> PathBuf {
    if config.remote_management_dir.as_os_str().is_empty() {
        config.download_dir.clone()
    } else {
        config.remote_management_dir.clone()
    }
}

fn effective_display_name(config: &Config) -> String {
    let name = config.remote_management_display_name.trim();
    if name.is_empty() {
        std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Nas-PC".to_string())
    } else {
        name.to_string()
    }
}

/// 列舉可用於區網連線的 IPv4 位址（供設定畫面顯示）。
pub fn list_lan_ipv4_addresses() -> Vec<String> {
    let mut addrs = Vec::new();
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            if let std::net::IpAddr::V4(ip) = iface.ip() {
                let octets = ip.octets();
                let is_private = octets[0] == 10
                    || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                    || (octets[0] == 192 && octets[1] == 168);
                if is_private {
                    addrs.push(ip.to_string());
                }
            }
        }
    }
    addrs.sort();
    addrs.dedup();
    addrs
}

/// 確保啟用遠端管理時具備 token。
pub fn ensure_token(config: &mut Config) {
    if config.remote_management_token.is_empty() {
        config.remote_management_token = uuid::Uuid::new_v4().to_string();
    }
}

// --- 遠端檔案操作（剪下／複製／貼上／刪除／重新命名，PC 端執行） ---

#[derive(Clone, Copy, PartialEq, Eq)]
enum RemoteClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone)]
struct RemoteClipboard {
    mode: RemoteClipboardMode,
    paths: Vec<String>,
}

static REMOTE_CLIPBOARD: OnceLock<SyncMutex<Option<RemoteClipboard>>> = OnceLock::new();

fn remote_clipboard_slot() -> &'static SyncMutex<Option<RemoteClipboard>> {
    REMOTE_CLIPBOARD.get_or_init(|| SyncMutex::new(None))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileOpBody {
    action: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    dest_path: String,
    #[serde(default)]
    new_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOpResponse {
    ok: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    clipboard_count: Option<usize>,
}

fn validate_entry_name(name: &str) -> anyhow::Result<String> {
    let n = name.trim();
    if n.is_empty() || n == "." || n == ".." {
        return Err(anyhow!("名稱不可為空"));
    }
    if n.contains(['/', '\\', '\0']) {
        return Err(anyhow!("名稱不可含 / 或 \\"));
    }
    Ok(n.to_string())
}

fn file_extension_name(name: &str) -> Option<String> {
    let idx = name.rfind('.')?;
    if idx == 0 {
        return None;
    }
    let ext = &name[idx..];
    if ext.len() < 2 {
        return None;
    }
    Some(ext.to_string())
}

fn user_has_explicit_extension(input: &str) -> bool {
    let idx = match input.rfind('.') {
        Some(i) if i > 0 => i,
        _ => return false,
    };
    !input[idx + 1..].trim().is_empty()
}

/// 未指定副檔名時保留原名副檔名（與 Android 一致）
fn resolve_rename_final_name(original_name: &str, user_input: &str, is_dir: bool) -> anyhow::Result<String> {
    let trimmed = validate_entry_name(user_input)?;
    if is_dir {
        return Ok(trimmed);
    }
    let Some(orig_ext) = file_extension_name(original_name) else {
        return Ok(trimmed);
    };
    if user_has_explicit_extension(&trimmed) {
        return Ok(trimmed);
    }
    let base = trimmed.trim_end_matches('.');
    Ok(format!("{base}{orig_ext}"))
}

fn normalize_rel_paths(paths: &[String]) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for p in paths {
        let t = p.trim().trim_start_matches(['/', '\\']).replace('\\', "/");
        if t.is_empty() {
            continue;
        }
        if !out.iter().any(|x| x == &t) {
            out.push(t);
        }
    }
    if out.is_empty() {
        return Err(anyhow!("未指定路徑"));
    }
    Ok(out)
}

fn copy_path_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_path_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

fn remove_path_recursive(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn move_path_item(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_path_recursive(src, dst)?;
            remove_path_recursive(src)?;
            Ok(())
        }
    }
}

fn unique_dest_path(dest_dir: &Path, base_name: &str) -> PathBuf {
    let mut candidate = dest_dir.join(base_name);
    if !path_exists(&candidate) {
        return candidate;
    }
    let path = Path::new(base_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| base_name.to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..1000 {
        candidate = dest_dir.join(format!("{stem} ({n}){ext}"));
        if !path_exists(&candidate) {
            return candidate;
        }
    }
    dest_dir.join(format!("{stem}_copy{ext}"))
}

fn is_subpath(parent_rel: &str, child_rel: &str) -> bool {
    if parent_rel.is_empty() {
        return true;
    }
    let p = parent_rel.trim_end_matches('/');
    let c = child_rel.trim_end_matches('/');
    c == p || c.starts_with(&format!("{p}/"))
}

fn execute_file_op(state: HttpState, body: FileOpBody) -> FileOpResponse {
    let action = body.action.trim().to_lowercase();
    match action.as_str() {
        "copy" | "cut" => {
            let paths = match normalize_rel_paths(&body.paths) {
                Ok(p) => p,
                Err(e) => return fail_op(e),
            };
            for rel in &paths {
                if state.resolve_under_share(rel).is_err() {
                    return fail_op(anyhow!("路徑不存在：`{rel}`"));
                }
            }
            let mode = if action == "cut" {
                RemoteClipboardMode::Cut
            } else {
                RemoteClipboardMode::Copy
            };
            let count = paths.len();
            *remote_clipboard_slot().lock() = Some(RemoteClipboard { mode, paths });
            let verb = if mode == RemoteClipboardMode::Cut {
                "剪下"
            } else {
                "複製"
            };
            FileOpResponse {
                ok: true,
                message: format!("已{verb} {count} 項"),
                clipboard_count: Some(count),
            }
        }
        "paste" => {
            let clip = remote_clipboard_slot().lock().clone();
            let Some(clip) = clip else {
                return fail_op(anyhow!("剪貼簿是空的，請先剪下或複製"));
            };
            let dest_rel = body.dest_path.trim().trim_start_matches(['/', '\\']).replace('\\', "/");
            let dest_dir = match state.resolve_browse_dir(&dest_rel) {
                Ok(d) => d,
                Err(e) => return fail_op(e),
            };
            let mut moved = 0usize;
            for rel in &clip.paths {
                if clip.mode == RemoteClipboardMode::Cut && is_subpath(rel, &dest_rel) {
                    return fail_op(anyhow!("無法貼到自身或子資料夾內"));
                }
                let src = match state.resolve_under_share(rel) {
                    Ok(p) => p,
                    Err(e) => return fail_op(e),
                };
                let name = src
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "item".to_string());
                let dst = unique_dest_path(&dest_dir, &name);
                let result = if clip.mode == RemoteClipboardMode::Cut {
                    move_path_item(&src, &dst)
                } else {
                    copy_path_recursive(&src, &dst)
                };
                if let Err(e) = result {
                    return fail_op(e.context(format!("貼上失敗：`{rel}`")));
                }
                moved += 1;
            }
            if clip.mode == RemoteClipboardMode::Cut {
                *remote_clipboard_slot().lock() = None;
            }
            FileOpResponse {
                ok: true,
                message: format!("已貼上 {moved} 項"),
                clipboard_count: None,
            }
        }
        "delete" | "delete_permanent" => {
            let paths = match normalize_rel_paths(&body.paths) {
                Ok(p) => p,
                Err(e) => return fail_op(e),
            };
            let mut deleted = 0usize;
            for rel in &paths {
                let target = match state.resolve_under_share(rel) {
                    Ok(p) => p,
                    Err(e) => return fail_op(e),
                };
                if let Err(e) = remove_path_recursive(&target) {
                    return fail_op(e.context(format!("永久刪除失敗：`{rel}`")));
                }
                deleted += 1;
            }
            FileOpResponse {
                ok: true,
                message: format!("已永久刪除 {deleted} 項"),
                clipboard_count: None,
            }
        }
        "delete_recycle" => {
            let paths = match normalize_rel_paths(&body.paths) {
                Ok(p) => p,
                Err(e) => return fail_op(e),
            };
            let mut deleted = 0usize;
            for rel in &paths {
                let target = match state.resolve_under_share(rel) {
                    Ok(p) => p,
                    Err(e) => return fail_op(e),
                };
                if let Err(e) = trash::delete(&target) {
                    return fail_op(anyhow!("移至資源回收桶失敗：`{rel}`：{e}"));
                }
                deleted += 1;
            }
            FileOpResponse {
                ok: true,
                message: format!("已將 {deleted} 項移至資源回收桶"),
                clipboard_count: None,
            }
        }
        "mkdir" => {
            let dest_rel = body.dest_path.trim().trim_start_matches(['/', '\\']).replace('\\', "/");
            let parent = match state.resolve_browse_dir(&dest_rel) {
                Ok(d) => d,
                Err(e) => return fail_op(e),
            };
            let name = match validate_entry_name(&body.new_name) {
                Ok(n) => n,
                Err(e) => return fail_op(e),
            };
            let dst = parent.join(&name);
            if path_exists(&dst) {
                return fail_op(anyhow!("資料夾已存在"));
            }
            if let Err(e) = std::fs::create_dir(&dst) {
                return fail_op(anyhow!("建立資料夾失敗：{e}"));
            }
            FileOpResponse {
                ok: true,
                message: format!("已建立資料夾 {name}"),
                clipboard_count: None,
            }
        }
        "rename" => {
            let paths = match normalize_rel_paths(&body.paths) {
                Ok(p) => p,
                Err(e) => return fail_op(e),
            };
            if paths.len() != 1 {
                return fail_op(anyhow!("重新命名一次只能選一項"));
            }
            let rel = &paths[0];
            let src = match state.resolve_under_share(rel) {
                Ok(p) => p,
                Err(e) => return fail_op(e),
            };
            let original_name = src
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let is_dir = src.is_dir();
            let new_name = match resolve_rename_final_name(&original_name, &body.new_name, is_dir) {
                Ok(n) => n,
                Err(e) => return fail_op(e),
            };
            let parent = match src.parent() {
                Some(p) => p,
                None => return fail_op(anyhow!("無法取得父目錄")),
            };
            let dst = parent.join(&new_name);
            if path_exists(&dst) {
                return fail_op(anyhow!("目標名稱已存在"));
            }
            if let Err(e) = move_path_item(&src, &dst) {
                return fail_op(e.context("重新命名失敗"));
            }
            FileOpResponse {
                ok: true,
                message: format!("已重新命名為 {new_name}"),
                clipboard_count: None,
            }
        }
        _ => fail_op(anyhow!("不支援的操作")),
    }
}

fn fail_op(err: impl std::fmt::Display) -> FileOpResponse {
    FileOpResponse {
        ok: false,
        message: err.to_string(),
        clipboard_count: None,
    }
}

async fn file_op_post_handler(
    State(state): State<HttpState>,
    Json(body): Json<FileOpBody>,
) -> Json<FileOpResponse> {
    let state = state.clone();
    let body = body;
    let resp = tokio::task::spawn_blocking(move || execute_file_op(state, body))
        .await
        .unwrap_or_else(|e| fail_op(anyhow!("{e}")));
    Json(resp)
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn upload_path_allowed_when_file_missing() {
        let dir = std::env::temp_dir().join(format!("gm_upload_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target =
            resolve_upload_path_under_share(&dir, "new_folder/photo.jpg").unwrap();
        assert!(target.to_string_lossy().ends_with("new_folder\\photo.jpg")
            || target.to_string_lossy().ends_with("new_folder/photo.jpg"));
        assert!(!path_exists(&target));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_state_upload_e_comic_mount_new_file() {
        let h = std::env::temp_dir().join(format!("gm_h_{}", uuid::Uuid::new_v4()));
        let i = std::env::temp_dir().join(format!("gm_i_{}", uuid::Uuid::new_v4()));
        let e = std::env::temp_dir().join(format!("gm_e_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&h).unwrap();
        std::fs::create_dir_all(&i).unwrap();
        std::fs::create_dir_all(&e).unwrap();
        let state = HttpState {
            mounts: vec![
                ShareMount {
                    label: "H--".to_string(),
                    display: "H:\\".to_string(),
                    root: h.clone(),
                },
                ShareMount {
                    label: "I--".to_string(),
                    display: "I:\\".to_string(),
                    root: i.clone(),
                },
                ShareMount {
                    label: "E--漫畫".to_string(),
                    display: "E:\\漫畫".to_string(),
                    root: e.clone(),
                },
            ],
            single_flat: false,
        };
        let target = state
            .resolve_upload_path("E--漫畫/IMG_20260603_120302.jpg")
            .unwrap();
        let e_canon = dunce::canonicalize(&e).unwrap_or(e.clone());
        assert!(target.starts_with(&e_canon));
        assert!(target.to_string_lossy().ends_with("IMG_20260603_120302.jpg"));
        assert!(!path_exists(&target));
        let resolved = resolve_upload_target_state(&state, "E--漫畫/IMG_20260603_120302.jpg", "overwrite")
            .unwrap();
        assert_eq!(resolved, target);
        let _ = std::fs::remove_dir_all(&h);
        let _ = std::fs::remove_dir_all(&i);
        let _ = std::fs::remove_dir_all(&e);
    }

    #[test]
    fn strip_mount_label_from_api_path() {
        let mount = ShareMount {
            label: "E--漫畫".to_string(),
            display: "E:\\漫畫".to_string(),
            root: PathBuf::from(r"E:\漫畫"),
        };
        assert_eq!(
            HttpState::strip_mount_prefix(&mount, "E--漫畫/IMG_test.jpg"),
            "IMG_test.jpg"
        );
        assert_eq!(
            HttpState::strip_mount_prefix(&mount, "IMG_test.jpg"),
            "IMG_test.jpg"
        );
    }

    #[test]
    fn ellipsis_in_filename_allowed() {
        let p = assemble_under_share(Path::new(r"C:\share"), r"日漫/chapter...end.zip").unwrap();
        assert!(p.to_string_lossy().ends_with("chapter...end.zip"));
    }

    #[test]
    fn dot_dot_segment_rejected() {
        let err = assemble_under_share(Path::new(r"C:\share"), r"日漫/../x").unwrap_err();
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn rename_keep_extension_when_omitted() {
        let out = resolve_rename_final_name("12345.zip", "123", false).unwrap();
        assert_eq!(out, "123.zip");
    }

    #[test]
    fn rename_explicit_extension() {
        let out = resolve_rename_final_name("12345.zip", "123.rar", false).unwrap();
        assert_eq!(out, "123.rar");
    }

    #[test]
    fn rename_folder_unchanged() {
        let out = resolve_rename_final_name("MyFolder", "NewName", true).unwrap();
        assert_eq!(out, "NewName");
    }
}
