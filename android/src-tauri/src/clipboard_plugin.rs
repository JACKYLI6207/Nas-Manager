use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime, State,
};

pub struct ClipboardPlugin<R: Runtime>(Option<tauri::plugin::PluginHandle<R>>);

pub fn init<R: Runtime>() -> TauriPlugin<R, ()> {
    Builder::<R, ()>::new("clipboard")
        .setup(|app, api| {
            let handle = init_handle(app, api);
            app.manage(ClipboardPlugin(handle));
            Ok(())
        })
        .build()
}

fn init_handle<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Option<tauri::plugin::PluginHandle<R>> {
    match api.register_android_plugin("com.gentleman.manager.android", "ClipboardPlugin") {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::warn!(message = %e, "ClipboardPlugin 初始化失敗");
            None
        }
    }
}

pub fn clipboard_plugin<R: Runtime>(app: &AppHandle<R>) -> crate::errors::CommandResult<&ClipboardPlugin<R>> {
    app.try_state::<ClipboardPlugin<R>>()
        .map(|s| s.inner())
        .ok_or_else(|| {
            crate::errors::CommandError::from(
                "剪貼簿不可用",
                anyhow::anyhow!("ClipboardPlugin 未載入"),
            )
        })
}

impl<R: Runtime> ClipboardPlugin<R> {
    pub fn copy_text(&self, text: &str) -> crate::errors::CommandResult<()> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            text: &'a str,
        }
        self.0
            .as_ref()
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "剪貼簿不可用",
                    anyhow::anyhow!("ClipboardPlugin 未載入"),
                )
            })?
            .run_mobile_plugin::<()>("copyText", Payload { text })
            .map_err(|e| crate::errors::CommandError::from("複製失敗", e))?;
        Ok(())
    }
}
