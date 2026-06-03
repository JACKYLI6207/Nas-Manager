use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeginWifiSessionResult {
    #[serde(default)]
    process_bind_ok: bool,
    #[serde(default)]
    multicast_ok: bool,
    #[serde(default)]
    message: String,
}

pub struct LanDiscovery<R: Runtime>(Option<tauri::plugin::PluginHandle<R>>);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiProbeResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub host: String,
    pub name: Option<String>,
    pub port: Option<u16>,
    #[serde(default)]
    pub remote_api: u32,
    pub error_kind: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubnetFoundHost {
    pub host: String,
    pub name: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubnetScanPluginResult {
    #[serde(default)]
    pub found: Vec<SubnetFoundHost>,
    #[serde(default)]
    pub log_lines: Vec<String>,
}

pub fn init<R: Runtime>() -> TauriPlugin<R, ()> {
    Builder::<R, ()>::new("lan-discovery")
        .setup(|app, api| {
            let handle = init_handle(app, api);
            app.manage(LanDiscovery(handle));
            Ok(())
        })
        .build()
}

fn init_handle<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Option<tauri::plugin::PluginHandle<R>> {
    match api.register_android_plugin("com.gentleman.manager.android", "LanDiscoveryPlugin") {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::warn!(message = %e, "LanDiscoveryPlugin 初始化失敗，區網掃描可能僅在模擬器可用");
            None
        }
    }
}

impl<R: Runtime> LanDiscovery<R> {
    pub fn is_available(&self) -> bool {
        self.0.is_some()
    }

    fn try_handle(&self) -> Option<&tauri::plugin::PluginHandle<R>> {
        self.0.as_ref()
    }

    pub fn begin_wifi_session(&self) -> String {
        self.try_handle()
            .and_then(|handle| {
                handle
                    .run_mobile_plugin::<BeginWifiSessionResult>("beginLanScan", ())
                    .ok()
            })
            .map(|r| {
                format!(
                    "{}（processBind={} multicast={}）",
                    r.message, r.process_bind_ok, r.multicast_ok
                )
            })
            .unwrap_or_else(|| "LanDiscoveryPlugin 不可用".to_string())
    }

    pub fn end_wifi_session(&self) {
        if let Some(handle) = self.try_handle() {
            let _ = handle.run_mobile_plugin::<serde_json::Value>("endLanScan", ());
        }
    }

    pub fn begin_scan(&self) {
        if let Some(handle) = self.try_handle() {
            let _ = handle.run_mobile_plugin::<serde_json::Value>("beginLanScan", ());
        }
    }

    pub fn end_scan(&self) {
        if let Some(handle) = self.try_handle() {
            let _ = handle.run_mobile_plugin::<serde_json::Value>("endLanScan", ());
        }
    }

    pub fn probe_health_on_wifi(
        &self,
        host: &str,
        port: u16,
        timeout_ms: u64,
    ) -> Option<WifiProbeResult> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            host: &'a str,
            port: u16,
            timeout_ms: u64,
        }
        self.try_handle()
            .and_then(|handle| {
                handle
                    .run_mobile_plugin::<WifiProbeResult>(
                        "probeHealthOnWifi",
                        Payload {
                            host,
                            port,
                            timeout_ms,
                        },
                    )
                    .ok()
            })
    }

    pub fn scan_subnet_on_wifi(
        &self,
        bind_ip: &str,
        netmask: &str,
        port: u16,
        timeout_ms: u64,
    ) -> Option<SubnetScanPluginResult> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            bind_ip: &'a str,
            netmask: &'a str,
            port: u16,
            timeout_ms: u64,
        }
        self.try_handle().and_then(|handle| {
            handle
                .run_mobile_plugin::<SubnetScanPluginResult>(
                    "scanSubnetHealthOnWifi",
                    Payload {
                        bind_ip,
                        netmask,
                        port,
                        timeout_ms,
                    },
                )
                .ok()
        })
    }
}

/// 遠端管理頁面期間維持 Wi‑Fi 綁定（Drop 時釋放）。
pub struct RemoteWifiSession<R: Runtime> {
    _app: AppHandle<R>,
}

impl<R: Runtime> RemoteWifiSession<R> {
    pub fn start(app: &AppHandle<R>) -> Self {
        Self { _app: app.clone() }
    }
}

impl<R: Runtime> Drop for RemoteWifiSession<R> {
    fn drop(&mut self) {
        if let Some(state) = self._app.try_state::<LanDiscovery<R>>() {
            state.end_wifi_session();
        }
    }
}
