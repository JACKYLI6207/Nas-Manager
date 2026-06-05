use serde::Deserialize;
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime, State,
};

pub struct LocalVideoPlayer<R: Runtime>(Option<tauri::plugin::PluginHandle<R>>);

#[derive(Debug, Deserialize)]
struct UriResponse {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

#[derive(Debug, Deserialize)]
struct PlayVideoResponse {
    #[serde(default)]
    cancelled: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    background: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PlayVideoResult {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub background: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct VideoPlaybackProgress {
    pub position_ms: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Deserialize)]
struct VideoProgressResponse {
    #[serde(default, rename = "positionMs")]
    position_ms: i64,
    #[serde(default, rename = "durationMs")]
    duration_ms: i64,
}

#[derive(Debug, Deserialize)]
struct BackgroundSessionResponse {
    #[serde(default, rename = "sessionJson")]
    session_json: Option<String>,
}

pub fn init<R: Runtime>() -> TauriPlugin<R, ()> {
    Builder::<R, ()>::new("local-video")
        .setup(|app, api| {
            let handle = init_handle(app, api);
            app.manage(LocalVideoPlayer(handle));
            Ok(())
        })
        .build()
}

fn init_handle<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Option<tauri::plugin::PluginHandle<R>> {
    match api.register_android_plugin("com.gentleman.manager.android", "LocalVideoPlugin") {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::warn!(message = %e, "LocalVideoPlugin 初始化失敗");
            None
        }
    }
}

impl<R: Runtime> LocalVideoPlayer<R> {
    fn try_handle(&self) -> Option<&tauri::plugin::PluginHandle<R>> {
        self.0.as_ref()
    }

    fn pick_uri_plugin(&self, command: &str) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<UriResponse>(command, ())
            .map_err(|e| crate::errors::CommandError::from("選擇檔案失敗", e))?;
        if res.cancelled {
            return Ok(None);
        }
        Ok(res.uri.filter(|s| !s.trim().is_empty()))
    }

    pub fn pick_local_video(&self) -> crate::errors::CommandResult<Option<String>> {
        self.pick_uri_plugin("pickLocalVideo")
    }

    pub fn pick_local_subtitle(&self) -> crate::errors::CommandResult<Option<String>> {
        self.pick_uri_plugin("pickLocalSubtitle")
    }

    pub fn play_local_video(
        &self,
        uri: &str,
        title: Option<&str>,
        subtitle_uris: &[String],
        pc_host: Option<&str>,
        pc_port: Option<u16>,
        pc_rel_path: Option<&str>,
        start_position_ms: Option<i64>,
        resume_only: Option<bool>,
    ) -> crate::errors::CommandResult<PlayVideoResult> {
        let start_position_ms = start_position_ms.unwrap_or(0).max(0);
        let resume_only = resume_only.unwrap_or(false);
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
            title: Option<&'a str>,
            #[serde(rename = "subtitleUris")]
            subtitle_uris: &'a [String],
            #[serde(rename = "pcHost")]
            pc_host: Option<&'a str>,
            #[serde(rename = "pcPort")]
            pc_port: Option<u16>,
            #[serde(rename = "pcRelPath")]
            pc_rel_path: Option<&'a str>,
            #[serde(rename = "startPositionMs")]
            start_position_ms: i64,
            #[serde(rename = "resumeOnly")]
            resume_only: bool,
        }
        let res = self
            .try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<PlayVideoResponse>(
                "playLocalVideo",
                Payload {
                    uri,
                    title,
                    subtitle_uris,
                    pc_host,
                    pc_port,
                    pc_rel_path,
                    start_position_ms,
                    resume_only,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("開啟播放器失敗", e))?;
        if res.cancelled {
            return Ok(PlayVideoResult {
                error: None,
                background: false,
            });
        }
        Ok(PlayVideoResult {
            error: res.error.filter(|s| !s.trim().is_empty()),
            background: res.background,
        })
    }

    pub fn get_background_playback_session(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<BackgroundSessionResponse>(
                "getBackgroundPlaybackSession",
                (),
            )
            .map_err(|e| crate::errors::CommandError::from("讀取播放狀態失敗", e))?;
        Ok(res
            .session_json
            .filter(|s| !s.trim().is_empty()))
    }

    pub fn get_video_playback_progress(
        &self,
        host: &str,
        port: u16,
        rel_path: &str,
    ) -> crate::errors::CommandResult<VideoPlaybackProgress> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            host: &'a str,
            port: u16,
            #[serde(rename = "relPath")]
            rel_path: &'a str,
        }
        let res = self
            .try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<VideoProgressResponse>(
                "getVideoPlaybackProgress",
                Payload { host, port, rel_path },
            )
            .map_err(|e| crate::errors::CommandError::from("讀取播放進度失敗", e))?;
        Ok(VideoPlaybackProgress {
            position_ms: res.position_ms.max(0),
            duration_ms: res.duration_ms.max(0),
        })
    }

    pub fn sync_stream_playlist(
        &self,
        jobs: &[crate::android_commands::StreamPlaylistJob],
        current_rel_path: Option<&str>,
    ) -> crate::errors::CommandResult<()> {
        #[derive(serde::Serialize)]
        struct JobPayload<'a> {
            host: &'a str,
            port: u16,
            #[serde(rename = "relPath")]
            rel_path: &'a str,
            title: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            jobs: Vec<JobPayload<'a>>,
            #[serde(rename = "currentRelPath")]
            current_rel_path: Option<&'a str>,
        }
        let job_payloads: Vec<JobPayload<'_>> = jobs
            .iter()
            .map(|j| JobPayload {
                host: &j.host,
                port: j.port,
                rel_path: &j.rel_path,
                title: &j.title,
            })
            .collect();
        #[derive(Debug, Deserialize)]
        struct OkResponse {
            #[serde(default)]
            ok: bool,
        }
        self.try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<OkResponse>(
                "syncStreamPlaylist",
                Payload {
                    jobs: job_payloads,
                    current_rel_path,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("同步播放列表失敗", e))?;
        Ok(())
    }

    pub fn stop_video_playback(&self) -> crate::errors::CommandResult<()> {
        #[derive(Debug, Deserialize)]
        struct StopResponse {
            #[serde(default)]
            ok: bool,
        }
        self.try_handle()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "影片播放不可用",
                    anyhow::anyhow!("LocalVideoPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<StopResponse>("stopVideoPlayback", ())
            .map_err(|e| crate::errors::CommandError::from("停止播放失敗", e))?;
        Ok(())
    }
}

pub fn local_video_player<'a, R: Runtime>(
    app: &'a AppHandle<R>,
) -> crate::errors::CommandResult<State<'a, LocalVideoPlayer<R>>> {
    app.try_state::<LocalVideoPlayer<R>>().ok_or_else(|| {
        crate::errors::CommandError::from(
            "影片播放不可用",
            anyhow::anyhow!("LocalVideoPlayer 未初始化"),
        )
    })
}

/// 組出 PC 遠端串流 URL（path_b64 避免特殊字元問題）；支援 ExoPlayer Range seek。
pub fn build_pc_stream_url(host: &str, port: u16, rel_path: &str) -> String {
    use base64::Engine;
    let path_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rel_path.as_bytes());
    format!("http://{host}:{port}/api/v1/stream?path_b64={path_b64}")
}
