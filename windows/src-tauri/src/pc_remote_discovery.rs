use std::net::Ipv4Addr;
use std::time::Duration;

use anyhow::Context;
use futures_util::future::join_all;
use futures_util::stream::{self, StreamExt};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::{AppHandle, Manager};
use tokio::net::UdpSocket;

const SERVICE_TYPE: &str = "_gentleman-manager._tcp.local.";
const UDP_DISCOVERY_PORT: u16 = 38765;
const DEFAULT_HTTP_PORT: u16 = 8765;
const DISCOVER_PACKET: &[u8] = b"GM_REMOTE_V1\nDISCOVER\n";
/// mDNS ??????? PC ???????
const MDNS_SCAN_SECS: u64 = 2;
const MDNS_EARLY_EXIT_GRACE_MS: u64 = 350;
/// ? LAN ?? UDP ????
const UDP_LISTEN_SECS: u64 = 2;
const UDP_EARLY_EXIT_GRACE_MS: u64 = 200;
/// ?? fallback ?? mDNS/UDP ??????
const SUBNET_PROBE_TIMEOUT_MS: u64 = 250;
const SUBNET_PROBE_CONCURRENCY: usize = 64;
const SUBNET_FALLBACK_MAX_SECS: u64 = 5;
const CONNECT_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredRemotePc {
    pub name: String,
    /// ????????? IPv4???????????
    pub hosts: Vec<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcConnectionResult {
    pub connected: bool,
    pub message: String,
    pub connected_host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcScanResult {
    pub pcs: Vec<DiscoveredRemotePc>,
    pub log: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcDirEntry {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub is_dir: bool,
    pub size: Option<u64>,
    #[serde(default)]
    pub disk_free_bytes: Option<u64>,
    #[serde(default)]
    pub disk_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcBrowseResult {
    pub path: String,
    #[serde(default)]
    pub path_display: Option<String>,
    pub entries: Vec<RemotePcDirEntry>,
    #[serde(default)]
    pub remote_api: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RemotePcFileItem {
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
struct ListFilesResponse {
    ok: bool,
    files: Vec<RemotePcFileItem>,
}

#[derive(Debug, Deserialize)]
struct BrowseResponse {
    ok: bool,
    path: String,
    #[serde(default)]
    path_display: Option<String>,
    entries: Vec<RemotePcDirEntry>,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    ok: bool,
    #[serde(default)]
    app: Option<String>,
    #[serde(default = "default_remote_api_v1")]
    remote_api: u32,
}

fn default_remote_api_v1() -> u32 {
    1
}

/// ?? PC ?? API ?????? `[`?????? v2??
pub async fn ensure_pc_remote_api_v2(host: &str, port: u16) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .get(format!("http://{host}:{port}/api/v1/health"))
        .send()
        .await
        .context("???? PC health")?;
    if !resp.status().is_success() {
        anyhow::bail!("PC health HTTP {}", resp.status());
    }
    let body: HealthResponse = resp.json().await.context("?? PC health ??")?;
    if !body.ok {
        anyhow::bail!("PC ????????");
    }
    if body.remote_api < 2 {
        anyhow::bail!(
            "PC ?????????remote_api={}??????????? Nas Manager Windows ?????v1.0.0 ?????????????????????",
            body.remote_api
        );
    }
    Ok(())
}

/// ?? PC ???????remote_api >= 3??
pub async fn ensure_pc_remote_api_v3(host: &str, port: u16) -> anyhow::Result<()> {
    ensure_pc_remote_api_v2(host, port).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .get(format!("http://{host}:{port}/api/v1/health"))
        .send()
        .await
        .context("???? PC health")?;
    let body: HealthResponse = resp.json().await.context("?? PC health ??")?;
    if body.remote_api < 3 {
        anyhow::bail!(
            "PC ??????????remote_api={}????? PC ? EXE ?????????",
            body.remote_api
        );
    }
    Ok(())
}

/// ?? PC health ? remote_api ??????? None??
pub async fn fetch_remote_pc_api_version(host: &str, port: u16) -> Option<u32> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .ok()?;
    let resp = client
        .get(format!("http://{host}:{port}/api/v1/health"))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: HealthResponse = resp.json().await.ok()?;
    if body.ok { Some(body.remote_api) } else { None }
}

/// ?? PC ?????????remote_api >= 5?HTTP Range??
pub async fn ensure_pc_remote_api_v5(host: &str, port: u16) -> anyhow::Result<()> {
    ensure_pc_remote_api_v2(host, port).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .get(format!("http://{host}:{port}/api/v1/health"))
        .send()
        .await
        .context("???? PC health")?;
    let body: HealthResponse = resp.json().await.context("?? PC health ??")?;
    if body.remote_api < 5 {
        anyhow::bail!(
            "PC ????????????remote_api={}????? PC ??????????",
            body.remote_api
        );
    }
    Ok(())
}

/// ?? PC ?????????remote_api >= 4??
pub async fn ensure_pc_remote_api_v4(host: &str, port: u16) -> anyhow::Result<()> {
    ensure_pc_remote_api_v2(host, port).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .get(format!("http://{host}:{port}/api/v1/health"))
        .send()
        .await
        .context("???? PC health")?;
    let body: HealthResponse = resp.json().await.context("?? PC health ??")?;
    if body.remote_api < 4 {
        anyhow::bail!(
            "PC ????????????remote_api={}????? PC ? EXE ?????????",
            body.remote_api
        );
    }
    Ok(())
}

pub async fn check_remote_upload_conflicts(
    host: &str,
    port: u16,
    paths: &[String],
) -> anyhow::Result<Vec<String>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/upload-exists"))
        .json(&json!({ "paths": paths }))
        .send()
        .await
        .context("?? PC ??????")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "?? PC ???? HTTP {status}{}",
            if body.is_empty() {
                String::new()
            } else {
                format!("?{body}")
            }
        );
    }
    #[derive(Deserialize)]
    struct ExistsResponse {
        conflicts: Vec<String>,
    }
    let body: ExistsResponse = resp.json().await.context("????????")?;
    Ok(body.conflicts)
}

struct ScanDiagnostics {
    lines: Vec<String>,
}

impl ScanDiagnostics {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn line(&mut self, msg: impl AsRef<str>) {
        self.lines.push(msg.as_ref().to_string());
    }

    fn section(&mut self, title: &str) {
        self.lines.push(String::new());
        self.lines.push(format!("--- {title} ---"));
    }

    fn into_log(self) -> String {
        self.lines.join("\n")
    }
}

struct LanInterface {
    name: String,
    ip: Ipv4Addr,
    netmask: Ipv4Addr,
    broadcast: Ipv4Addr,
}

fn is_cellular_interface(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("rmnet")
        || n.starts_with("ccmni")
        || n.starts_with("pdp")
        || n.starts_with("wwan")
        || n.starts_with("usb")
}

fn list_lan_interfaces() -> Vec<LanInterface> {
    let mut out = Vec::new();
    let Ok(ifaces) = get_if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        if iface.is_loopback() || is_cellular_interface(&iface.name) {
            continue;
        }
        let get_if_addrs::IfAddr::V4(v4) = iface.addr else {
            continue;
        };
        if !v4.ip.is_private() {
            continue;
        }
        let broadcast = v4.broadcast.unwrap_or_else(|| {
            let mask = u32::from(v4.netmask);
            Ipv4Addr::from((u32::from(v4.ip) & mask) | !mask)
        });
        out.push(LanInterface {
            name: iface.name,
            ip: v4.ip,
            netmask: v4.netmask,
            broadcast,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn ipv4_host_range(ip: Ipv4Addr, netmask: Ipv4Addr) -> Vec<Ipv4Addr> {
    let ip_u = u32::from(ip);
    let mask = u32::from(netmask);
    if mask == 0 {
        return Vec::new();
    }
    let network = ip_u & mask;
    let broadcast = network | !mask;
    if broadcast <= network + 1 {
        return Vec::new();
    }
    (network + 1..broadcast)
        .map(Ipv4Addr::from)
        .filter(|host| *host != ip)
        .collect()
}

fn describe_local_interfaces() -> Vec<String> {
    let mut lines = Vec::new();
    let Ok(ifaces) = get_if_addrs::get_if_addrs() else {
        lines.push("??????????".to_string());
        return lines;
    };
    let mut count = 0usize;
    for iface in ifaces {
        if iface.is_loopback() {
            continue;
        }
        let get_if_addrs::IfAddr::V4(v4) = iface.addr else {
            continue;
        };
        count += 1;
        let kind = if is_cellular_interface(&iface.name) {
            "????"
        } else if v4.ip.is_private() {
            "LAN"
        } else {
            "??"
        };
        let bc = v4
            .broadcast
            .or_else(|| {
                let mask = u32::from(v4.netmask);
                if mask == 0 {
                    None
                } else {
                    Some(Ipv4Addr::from((u32::from(v4.ip) & mask) | !mask))
                }
            })
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "{} [{kind}]: ip={} mask={} broadcast={}",
            iface.name, v4.ip, v4.netmask, bc
        ));
    }
    if count == 0 {
        lines.push("????? IPv4 ???????? Wi?Fi?".to_string());
    }
    lines
}

/// ?????????????? PC?mDNS + UDP ????
pub async fn scan_lan_remote_pcs<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> anyhow::Result<RemotePcScanResult> {
    let mut log = ScanDiagnostics::new();
    log.line("=== Nas Manager ???? LOG ===");
    log.line(format!("mDNS ?? {MDNS_SCAN_SECS}s??????"));
    log.line(format!("UDP ?? {UDP_LISTEN_SECS}s???"));
    log.line(format!("mDNS ???{SERVICE_TYPE}"));
    log.line(format!("UDP ????{UDP_DISCOVERY_PORT}"));

    #[cfg(target_os = "android")]
    {
        let plugin_ok = app
            .try_state::<crate::lan_discovery::LanDiscovery<R>>()
            .map(|state| state.is_available())
            .unwrap_or(false);
        log.line(format!(
            "LanDiscoveryPlugin?MulticastLock??{}",
            if plugin_ok {
                "????????? acquire"
            } else {
                "?????? mDNS ??????"
            }
        ));
    }

    log.section("??????");
    for line in describe_local_interfaces() {
        log.line(line);
    }

    let lan_ifaces = list_lan_interfaces();

    let (mdns_result, udp_result) = tokio::join!(
        tauri::async_runtime::spawn_blocking(scan_mdns_with_log),
        scan_udp_broadcast_with_log()
    );
    let (mdns_pcs, mdns_lines) = mdns_result.unwrap_or_else(|_| {
        (
            Vec::new(),
            vec!["mDNS ???????".to_string()],
        )
    });
    let (udp_pcs, udp_lines) = udp_result;

    log.section("mDNS");
    for line in mdns_lines {
        log.line(line);
    }
    log.section("UDP ??");
    for line in udp_lines {
        log.line(line);
    }

    let mut found = mdns_pcs;
    for pc in udp_pcs {
        merge_discovered(&mut found, pc);
    }

    let (subnet_pcs, subnet_lines) = if found.is_empty() {
        log.line("mDNS/UDP ??? ? ???? health fallback");
        match tokio::time::timeout(
            Duration::from_secs(SUBNET_FALLBACK_MAX_SECS),
            scan_subnet_health_with_log(app, &lan_ifaces),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => (
                Vec::new(),
                vec![format!(
                    "?? fallback ???>{SUBNET_FALLBACK_MAX_SECS}s????????? PC IP"
                )],
            ),
        }
    } else {
        (
            Vec::new(),
            vec![
                "?mDNS ? UDP ??? PC????? 253 IP ????????".to_string(),
            ],
        )
    };
    log.section("?? health ??");
    for line in subnet_lines {
        log.line(line);
    }

    for pc in subnet_pcs {
        merge_discovered(&mut found, pc);
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));

    log.section("????");
    if found.is_empty() {
        log.line("??? PC");
        log.line("???? PC ???????????? ? ?? Wi?Fi ? ?? AP ?? ? Windows ?????????");
    } else {
        for pc in &found {
            log.line(format!(
                "? {} ? {}:{}",
                pc.name,
                pc.hosts.join(" / "),
                pc.port
            ));
        }
    }

    Ok(RemotePcScanResult {
        pcs: found,
        log: log.into_log(),
    })
}

fn merge_discovered(found: &mut Vec<DiscoveredRemotePc>, pc: DiscoveredRemotePc) {
    let mut pc = pc;
    pc.hosts.retain(|h| !is_emulator_only_host(h));
    if pc.hosts.is_empty() {
        return;
    }
    pc.hosts.sort();
    pc.hosts.dedup();

    if let Some(existing) = found
        .iter_mut()
        .find(|e| e.port == pc.port && hosts_overlap(&e.hosts, &pc.hosts))
    {
        existing.name = prefer_pc_display_name(&existing.name, &pc.name);
        for host in pc.hosts {
            if !existing.hosts.contains(&host) {
                existing.hosts.push(host);
            }
        }
        existing.hosts.sort();
        existing.hosts.dedup();
        return;
    }

    if let Some(existing) = found
        .iter_mut()
        .find(|e| e.name == pc.name && e.port == pc.port)
    {
        for host in pc.hosts {
            if !existing.hosts.contains(&host) {
                existing.hosts.push(host);
            }
        }
        existing.hosts.sort();
        existing.hosts.dedup();
    } else {
        found.push(pc);
    }
}

/// Android ??????????????????????
fn is_emulator_only_host(host: &str) -> bool {
    host.starts_with("10.0.2.")
}

fn hosts_overlap(a: &[String], b: &[String]) -> bool {
    a.iter().any(|h| b.contains(h))
}

fn prefer_pc_display_name(current: &str, incoming: &str) -> String {
    const GENERIC: [&str; 3] = ["Nas Manager", "Nas-Manager-PC", "Nas Manager PC"];
    let is_generic = |s: &str| GENERIC.iter().any(|g| s.eq_ignore_ascii_case(g));
    if is_generic(current) && !is_generic(incoming) {
        incoming.to_string()
    } else if !is_generic(current) {
        current.to_string()
    } else {
        incoming.to_string()
    }
}

fn scan_mdns_with_log() -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    let mut lines = Vec::new();
    let mdns = match ServiceDaemon::new() {
        Ok(m) => m,
        Err(err) => {
            lines.push(format!("?? mDNS ??????{err}"));
            return (Vec::new(), lines);
        }
    };
    let receiver = match mdns.browse(SERVICE_TYPE) {
        Ok(r) => r,
        Err(err) => {
            lines.push(format!("???????{err}"));
            let _ = mdns.shutdown();
            return (Vec::new(), lines);
        }
    };
    lines.push("?????".to_string());

    let mut found = Vec::new();
    let mut resolved = 0usize;
    let mut service_found = 0usize;
    let deadline = std::time::Instant::now() + Duration::from_secs(MDNS_SCAN_SECS);
    let mut last_resolve: Option<std::time::Instant> = None;

    while std::time::Instant::now() < deadline {
        if last_resolve.is_some_and(|t| {
            !found.is_empty() && t.elapsed() >= Duration::from_millis(MDNS_EARLY_EXIT_GRACE_MS)
        }) {
            lines.push("???? PC????? mDNS?".to_string());
            break;
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                resolved += 1;
                let pc = parse_mdns_info(&info);
                lines.push(format!(
                    "ServiceResolved #{resolved}: {} ? {}:{}",
                    pc.name,
                    pc.hosts.join("/"),
                    pc.port
                ));
                merge_discovered(&mut found, pc);
                last_resolve = Some(std::time::Instant::now());
            }
            Ok(ServiceEvent::ServiceFound(name, _)) => {
                service_found += 1;
                lines.push(format!("ServiceFound #{service_found}: {name}"));
            }
            Ok(_) => {}
            Err(_) => continue,
        }
    }

    let _ = mdns.shutdown();
    lines.push(format!(
        "mDNS ???ServiceFound={service_found} ServiceResolved={resolved} ??={}",
        found.len()
    ));
    (found, lines)
}

fn parse_mdns_info(info: &mdns_sd::ServiceInfo) -> DiscoveredRemotePc {
    let name = info.get_fullname().to_string();
    let display_name = name
        .split('.')
        .next()
        .unwrap_or(&name)
        .to_string();
    let mut hosts: Vec<String> = info
        .get_addresses()
        .iter()
        .filter(|ip| ip.is_ipv4())
        .map(|ip| ip.to_string())
        .collect();
    hosts.sort();
    hosts.dedup();
    let port = info.get_port();
    DiscoveredRemotePc {
        name: display_name,
        hosts,
        port,
    }
}


async fn scan_udp_on_interface(
    iface: &LanInterface,
    listen_secs: u64,
) -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    let mut lines = Vec::new();
    lines.push(format!(
        "?? {}?bind {} ? broadcast {}:{}",
        iface.name, iface.ip, iface.broadcast, UDP_DISCOVERY_PORT
    ));

    let socket = match UdpSocket::bind((iface.ip, 0)).await {
        Ok(s) => s,
        Err(err) => {
            lines.push(format!("  bind ???{err}"));
            return (Vec::new(), lines);
        }
    };
    if let Err(err) = socket.set_broadcast(true) {
        lines.push(format!("  ?? broadcast ???{err}"));
        return (Vec::new(), lines);
    }

    let target = format!("{}:{UDP_DISCOVERY_PORT}", iface.broadcast);
    for round in 0..2 {
        match socket.send_to(DISCOVER_PACKET, &target).await {
            Ok(n) => {
                if round == 0 {
                    lines.push(format!("  ?? DISCOVER ? {target} ({n} bytes)"));
                }
            }
            Err(err) => lines.push(format!("  ???? ? {target}?{err}")),
        }
        if round < 1 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    let mut found = Vec::new();
    let mut buf = [0_u8; 512];
    let mut reply_count = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(listen_secs);
    let mut early_stop_at: Option<tokio::time::Instant> = None;

    while tokio::time::Instant::now() < deadline {
        if early_stop_at.is_some_and(|t| tokio::time::Instant::now() >= t) {
            lines.push("  ???? PC ??????? UDP ???".to_string());
            break;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(
            remaining.min(Duration::from_millis(200)),
            socket.recv_from(&mut buf),
        )
        .await
        {
            Ok(Ok((len, peer))) => {
                reply_count += 1;
                let pc = parse_udp_reply(&buf[..len]);
                lines.push(format!(
                    "  UDP ?? #{reply_count} from {peer} ({len} bytes)"
                ));
                if pc.hosts.is_empty() || pc.port == 0 {
                    lines.push("    ??????".to_string());
                } else {
                    lines.push(format!(
                        "    ? {} {}:{}",
                        pc.name,
                        pc.hosts.join("/"),
                        pc.port
                    ));
                }
                merge_discovered(&mut found, pc);
                if !found.is_empty() && early_stop_at.is_none() {
                    early_stop_at = Some(
                        tokio::time::Instant::now()
                            + Duration::from_millis(UDP_EARLY_EXIT_GRACE_MS),
                    );
                }
            }
            Ok(Err(err)) => lines.push(format!("  recv ???{err}")),
            Err(_) => continue,
        }
    }
    lines.push(format!("  ????? {reply_count} ?"));
    (found, lines)
}

async fn scan_udp_broadcast_with_log() -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    let interfaces = list_lan_interfaces();
    if interfaces.is_empty() {
        return (
            Vec::new(),
            vec![
                "?? private LAN ?????????? rmnet ??".to_string(),
                "?????????????????? Wi?Fi".to_string(),
            ],
        );
    }

    let mut lines = vec![format!(
        "??? LAN ?? {} ??bind ? Wi?Fi IP?????? rmnet?",
        interfaces.len()
    )];
    let listen_secs = UDP_LISTEN_SECS;
    let scan_results = join_all(
        interfaces
            .iter()
            .map(|iface| scan_udp_on_interface(iface, listen_secs)),
    )
    .await;
    let mut found = Vec::new();
    for (iface_found, iface_lines) in scan_results {
        lines.extend(iface_lines);
        for pc in iface_found {
            merge_discovered(&mut found, pc);
        }
    }
    lines.push(format!("UDP ???{} ?", found.len()));
    (found, lines)
}

async fn probe_subnet_health(ip: Ipv4Addr) -> Option<DiscoveredRemotePc> {
    let ip_str = ip.to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(SUBNET_PROBE_TIMEOUT_MS))
        .build()
        .ok()?;
    let url = format!("http://{ip_str}:{DEFAULT_HTTP_PORT}/api/v1/health");
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: HealthResponse = resp.json().await.ok()?;
    if !body.ok {
        return None;
    }
    Some(DiscoveredRemotePc {
        name: body
            .app
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("PC ({ip_str})")),
        hosts: vec![ip_str],
        port: DEFAULT_HTTP_PORT,
    })
}

async fn scan_subnet_health_with_log<R: tauri::Runtime>(
    app: &AppHandle<R>,
    interfaces: &[LanInterface],
) -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    #[cfg(target_os = "android")]
    {
        if app
            .try_state::<crate::lan_discovery::LanDiscovery<R>>()
            .map(|d| d.is_available())
            .unwrap_or(false)
        {
            return scan_subnet_health_on_wifi_plugin(app, interfaces).await;
        }
    }
    scan_subnet_health_reqwest(interfaces).await
}

#[cfg(target_os = "android")]
async fn scan_subnet_health_on_wifi_plugin<R: tauri::Runtime>(
    app: &AppHandle<R>,
    interfaces: &[LanInterface],
) -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    let mut lines = Vec::new();
    let mut found = Vec::new();
    if interfaces.is_empty() {
        lines.push("?? LAN ??????".to_string());
        return (found, lines);
    }
    lines.push(format!(
        "?? :{DEFAULT_HTTP_PORT}/health?Android Wi?Fi Network ????? {SUBNET_PROBE_TIMEOUT_MS}ms?"
    ));

    for iface in interfaces {
        let app = app.clone();
        let bind_ip = iface.ip.to_string();
        let netmask = iface.netmask.to_string();
        let name = iface.name.clone();
        lines.push(format!("{}?{} / {}?", name, bind_ip, netmask));
        let bind_ip_for_scan = bind_ip.clone();
        let netmask_for_scan = netmask.clone();
        let plugin_result = tauri::async_runtime::spawn_blocking(move || {
            app.try_state::<crate::lan_discovery::LanDiscovery<R>>()
                .and_then(|discovery| {
                    discovery.scan_subnet_on_wifi(
                        &bind_ip_for_scan,
                        &netmask_for_scan,
                        DEFAULT_HTTP_PORT,
                        SUBNET_PROBE_TIMEOUT_MS,
                    )
                })
        })
        .await
        .ok()
        .flatten();

        match plugin_result {
            Some(result) => {
                lines.extend(result.log_lines);
                for h in result.found {
                    let pc = DiscoveredRemotePc {
                        name: h.name,
                        hosts: vec![h.host],
                        port: h.port,
                    };
                    merge_discovered(&mut found, pc);
                }
            }
            None => {
                lines.push(format!("{name}?Wi?Fi ??????????"));
            }
        }
    }
    (found, lines)
}

async fn scan_subnet_health_reqwest(
    interfaces: &[LanInterface],
) -> (Vec<DiscoveredRemotePc>, Vec<String>) {
    let mut lines = Vec::new();
    let mut found = Vec::new();
    if interfaces.is_empty() {
        lines.push("?? LAN ??????".to_string());
        return (found, lines);
    }

    lines.push(format!(
        "??/mDNS ?????????? :{DEFAULT_HTTP_PORT}/health??? {SUBNET_PROBE_TIMEOUT_MS}ms?"
    ));

    for iface in interfaces {
        let hosts = ipv4_host_range(iface.ip, iface.netmask);
        if hosts.is_empty() {
            lines.push(format!("{}???????", iface.name));
            continue;
        }
        lines.push(format!(
            "{}??? {} ?????????{} / {}??",
            iface.name,
            hosts.len(),
            iface.ip,
            iface.netmask
        ));

        let mut stream = stream::iter(hosts)
            .map(|target| async move { probe_subnet_health(target).await })
            .buffer_unordered(SUBNET_PROBE_CONCURRENCY);
        let mut hit_count = 0usize;
        while let Some(pc_opt) = stream.next().await {
            if let Some(pc) = pc_opt {
                hit_count += 1;
                let host = pc.hosts.first().cloned().unwrap_or_default();
                lines.push(format!("  health OK: {host} ? {}:{}", pc.name, pc.port));
                merge_discovered(&mut found, pc);
                break;
            }
        }
        lines.push(format!("  ????? {hit_count} ?"));
    }
    (found, lines)
}

fn parse_udp_reply(data: &[u8]) -> DiscoveredRemotePc {
    let mut name = String::new();
    let mut port = 0_u16;
    let mut hosts = Vec::new();
    if let Ok(text) = std::str::from_utf8(data) {
        let mut lines = text.lines();
        if lines.next() == Some("GM_REMOTE_V1") && lines.next() == Some("OK") {
            name = lines.next().unwrap_or("").to_string();
            if let Some(p) = lines.next() {
                port = p.parse().unwrap_or(0);
            }
            for line in lines {
                let line = line.trim();
                if line.parse::<std::net::Ipv4Addr>().is_ok() {
                    hosts.push(line.to_string());
                }
            }
        }
    }
    hosts.sort();
    hosts.dedup();
    DiscoveredRemotePc { name, hosts, port }
}

/// Tailscale CGNAT?100.64.0.0/10?? MagicDNS??????????? VPN????? Wi?Fi ???
fn is_overlay_vpn_host(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.');
    if host.ends_with(".ts.net") || host.ends_with(".tailscale.net") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        let o = ip.octets();
        return o[0] == 100 && (64..=127).contains(&o[1]);
    }
    false
}

fn host_connection_priority(host: &str) -> u8 {
    if host.starts_with("192.168.") {
        0
    } else if is_overlay_vpn_host(host) {
        1
    } else if host.starts_with("10.0.2.") {
        9
    } else if host.starts_with("10.") {
        2
    } else if host.starts_with("172.") {
        3
    } else {
        4
    }
}

fn sort_hosts_for_connection(mut hosts: Vec<String>) -> Vec<String> {
    hosts.sort_by_key(|h| host_connection_priority(h));
    hosts
}

#[cfg(target_os = "android")]
fn format_wifi_probe_error(probe: &crate::lan_discovery::WifiProbeResult) -> String {
    let kind = probe.error_kind.as_deref().unwrap_or("other");
    let detail = probe.error.as_deref().unwrap_or("");
    match kind {
        "connection_refused" => {
            format!("??????{detail}??PC ????????? {DEFAULT_HTTP_PORT} ???")
        }
        "timeout" => format!("?????{detail}????? AP ??? PC ??????"),
        "no_route" => format!("????{detail}????????????? PC ?? Wi?Fi"),
        "no_wifi" => {
            format!("??? Wi?Fi Network?{detail}?????? Tailscale IP?100.x.x.x???? Wi?Fi")
        }
        "bind_failed" => format!(
            "Wi?Fi ?????{detail}????????????????? Tailscale VPN?"
        ),
        "http_error" => format!("HTTP ???{detail}?"),
        _ => format!("?????{kind}?{detail}?"),
    }
}

/// Wi?Fi ??????????????????????? IP ????
#[cfg(target_os = "android")]
fn wifi_probe_should_fallback(probe: &crate::lan_discovery::WifiProbeResult) -> bool {
    if probe.ok {
        return false;
    }
    match probe.error_kind.as_deref() {
        Some("no_wifi") | Some("bind_failed") => true,
        Some("other") => {
            let detail = probe.error.as_deref().unwrap_or("");
            detail.contains("EPERM")
                || detail.contains("Binding socket to network")
                || detail.contains("bindProcessToNetwork")
        }
        _ => false,
    }
}

/// ?????? IP????????????
/// `skip_wifi_bind`????? IP ?? true??? Wi?Fi ????????????? Tailscale VPN??
pub async fn test_remote_pc_connection<R: tauri::Runtime>(
    app: Option<&AppHandle<R>>,
    hosts: Vec<String>,
    port: u16,
    skip_wifi_bind: bool,
) -> RemotePcConnectionResult {
    let hosts: Vec<String> = sort_hosts_for_connection(
        hosts
            .into_iter()
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty())
            .collect(),
    );
    if hosts.is_empty() {
        return RemotePcConnectionResult {
            connected: false,
            message: "?????? IP".to_string(),
            connected_host: None,
        };
    }

    let mut last_msg = String::new();
    for host in &hosts {
        let use_overlay_route = is_overlay_vpn_host(host);

        #[cfg(target_os = "android")]
        if !skip_wifi_bind && !use_overlay_route {
            if let Some(app_handle) = app {
                if let Some(discovery) =
                    app_handle.try_state::<crate::lan_discovery::LanDiscovery<R>>()
                {
                    if discovery.is_available() {
                        let app_for_blocking = app_handle.clone();
                        let host_owned = host.clone();
                        let probe = tauri::async_runtime::spawn_blocking(move || {
                            app_for_blocking
                                .try_state::<crate::lan_discovery::LanDiscovery<R>>()
                                .and_then(|d| {
                                    d.probe_health_on_wifi(
                                        &host_owned,
                                        port,
                                        SUBNET_PROBE_TIMEOUT_MS,
                                    )
                                })
                        })
                        .await
                        .ok()
                        .flatten();
                        if let Some(probe) = probe {
                            if probe.ok {
                                return RemotePcConnectionResult {
                                    connected: true,
                                    message: if hosts.len() > 1 {
                                        format!("????{host}?Wi?Fi ???")
                                    } else {
                                        "????Wi?Fi ???".to_string()
                                    },
                                    connected_host: Some(host.clone()),
                                };
                            }
                            last_msg = format_wifi_probe_error(&probe);
                            // Wi?Fi ??????? Wi?Fi / Tailscale ?? VPN / EPERM?? ???? HTTP
                            if !wifi_probe_should_fallback(&probe) {
                                continue;
                            }
                        }
                    }
                }
            }
        }

        let result = probe_health(host, port).await;
        if result.connected {
            return RemotePcConnectionResult {
                connected: true,
                message: if use_overlay_route {
                    if hosts.len() > 1 {
                        format!("????{host}????VPN?")
                    } else {
                        "???????VPN?".to_string()
                    }
                } else if hosts.len() > 1 {
                    format!("????{host}?")
                } else {
                    "???".to_string()
                },
                connected_host: Some(host.clone()),
            };
        }
        last_msg = result.message;
    }

    RemotePcConnectionResult {
        connected: false,
        message: format_connection_failure(&hosts, port, &last_msg),
        connected_host: None,
    }
}

/// ?? PC ??????????`path` ???? PC ????????????????
pub async fn list_remote_pc_directory(
    host: &str,
    port: u16,
    path: &str,
) -> anyhow::Result<RemotePcBrowseResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/browse"))
        .json(&json!({ "path": path }))
        .send()
        .await
        .context("???? PC")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status}{}", if body.is_empty() { String::new() } else { format!("?{body}") });
    }
    let text = resp.text().await.context("????????")?;
    let body: BrowseResponse = serde_json::from_str(&text).context("????????")?;
    if !body.ok {
        anyhow::bail!("PC ????");
    }
    let remote_api = fetch_remote_pc_api_version(host, port).await;
    Ok(RemotePcBrowseResult {
        path: body.path,
        path_display: body.path_display,
        entries: body.entries,
        remote_api,
    })
}

/// ?? PC ????????????????????????
pub async fn list_remote_pc_files(
    host: &str,
    port: u16,
    path: &str,
) -> anyhow::Result<Vec<RemotePcFileItem>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("?? HTTP ?????")?;
    let resp = client
        .post(format!("http://{host}:{port}/api/v1/list-files"))
        .json(&json!({ "path": path }))
        .send()
        .await
        .context("???? PC")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("HTTP {status}{}", if body.is_empty() { String::new() } else { format!("?{body}") });
    }
    let text = resp.text().await.context("????????")?;
    let body: ListFilesResponse = serde_json::from_str(&text).context("????????")?;
    if !body.ok {
        anyhow::bail!("PC ????");
    }
    Ok(body.files)
}

fn format_connection_failure(hosts: &[String], port: u16, detail: &str) -> String {
    let ips = hosts.join("?");
    if detail.contains("connection refused") || detail.contains("????") {
        return format!(
            "???? {ips}:{port}?PC ??? HTTP ??? Windows ?????????? PC ????????????????? Nas Manager ?????"
        );
    }
    if detail.contains("timed out") || detail.contains("??") {
        return format!(
            "?????{ips}:{port}???????? PC ?? Wi?Fi ???????????AP ???????????"
        );
    }
    format!("??????? {ips}??{detail}")
}

async fn probe_health(host: &str, port: u16) -> RemotePcConnectionResult {
    let url = format!("http://{host}:{port}/api/v1/health");
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            return RemotePcConnectionResult {
                connected: false,
                message: format!("?? HTTP ??????{err}"),
                connected_host: None,
            };
        }
    };

    match client.get(&url).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return RemotePcConnectionResult {
                    connected: false,
                    message: format!("HTTP {}", resp.status()),
                    connected_host: None,
                };
            }
            match resp.text().await {
                Ok(text) => match serde_json::from_str::<HealthResponse>(&text) {
                    Ok(body) if body.ok => {
                        let message = if body.remote_api >= 2 {
                            "???".to_string()
                        } else {
                            "????PC ????????? [ ] ???????? EXE?"
                                .to_string()
                        };
                        RemotePcConnectionResult {
                            connected: true,
                            message,
                            connected_host: Some(host.to_string()),
                        }
                    }
                    Ok(_) => RemotePcConnectionResult {
                        connected: false,
                        message: "????".to_string(),
                        connected_host: None,
                    },
                    Err(err) => RemotePcConnectionResult {
                        connected: false,
                        message: format!("???????{err}"),
                        connected_host: None,
                    },
                },
                Err(err) => RemotePcConnectionResult {
                    connected: false,
                    message: format!("???????{err}"),
                    connected_host: None,
                },
            }
        }
        Err(err) => RemotePcConnectionResult {
            connected: false,
            message: err.to_string(),
            connected_host: None,
        },
    }
}
