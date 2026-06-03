use tauri::AppHandle;

use crate::{
    config::Config,
    errors::{CommandError, CommandResult},
    extensions::AppHandleExt,
    logger,
};

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_config(app: AppHandle) -> Config {
    app.get_config().read().clone()
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn save_config(app: AppHandle, config: Config) -> CommandResult<()> {
    let config_state = app.get_config();
    let enable_file_logger = config.enable_file_logger;
    let file_logger_changed = config_state.read().enable_file_logger != enable_file_logger;

    {
        let mut config = config;
        crate::share_roots::normalize_remote_share_config(&mut config);
        if config.remote_management_enabled {
            crate::remote_management::ensure_token(&mut config);
        }
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("儲存設定失敗", err))?;
    }

    let saved_config = app.get_config().read().clone();
    if saved_config.remote_management_enabled {
        if !crate::remote_management::is_running_with_config(&saved_config) {
            crate::remote_management::sync_async(&app).await;
        } else {
            crate::remote_management::clear_last_error();
        }
    } else {
        crate::remote_management::sync_async(&app).await;
    }

    if file_logger_changed {
        if enable_file_logger {
            logger::reload_file_logger()
                .map_err(|err| CommandError::from("重新加載檔案日誌失敗", err))?;
        } else {
            logger::disable_file_logger()
                .map_err(|err| CommandError::from("禁用檔案日誌失敗", err))?;
        }
    }

    if saved_config.remote_management_enabled {
        let config = app.get_config().read().clone();
        let status = crate::remote_management::get_status(&config);
        if !status.running {
            let detail = status
                .last_error
                .unwrap_or_else(|| "未知原因".to_string());
            return Err(CommandError::from(
                "遠端管理啟動失敗",
                anyhow::anyhow!(detail),
            ));
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn get_remote_management_status(
    app: AppHandle,
) -> crate::remote_management::RemoteManagementStatus {
    let config = app.get_config().read().clone();
    crate::remote_management::get_status(&config)
}

#[tauri::command]
#[specta::specta]
pub async fn restart_remote_management(
    app: AppHandle,
) -> CommandResult<crate::remote_management::RemoteManagementStatus> {
    {
        let config_state = app.get_config();
        let mut config = config_state.write();
        if !config.remote_management_enabled {
            return Ok(crate::remote_management::get_status(&*config));
        }
        crate::remote_management::ensure_token(&mut config);
        config
            .save(&app)
            .map_err(|err| CommandError::from("儲存設定失敗", err))?;
    }
    crate::remote_management::force_start_async(&app).await;
    let config = app.get_config().read().clone();
    Ok(crate::remote_management::get_status(&config))
}
