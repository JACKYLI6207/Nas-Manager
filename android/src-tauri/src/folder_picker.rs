use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime, State,
};

pub struct FolderPicker<R: Runtime>(Option<tauri::plugin::PluginHandle<R>>);

#[derive(Debug, Deserialize)]
struct UriResponse {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

fn uri_pick_result(res: UriResponse) -> Option<String> {
    if res.cancelled {
        return None;
    }
    res.uri.filter(|s| !s.trim().is_empty())
}

#[derive(Debug, Deserialize)]
struct TextResponse {
    text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidUploadFile {
    pub uri: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidTxtFile {
    pub uri: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSnapshotFile {
    pub cate_id: Option<i64>,
    pub label: String,
    pub file_path: String,
    #[serde(default)]
    pub meta_id: String,
    #[serde(default)]
    pub saved_at: String,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub total_pages: i64,
    #[serde(default)]
    pub scan_completion_percent: i64,
    #[serde(default)]
    pub scan_completed_pages: i64,
    #[serde(default)]
    pub modified_ms: Option<i64>,
    #[serde(default)]
    pub scan_target_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SnapshotFilesResponse {
    files: Vec<AndroidSnapshotFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidReaderSource {
    pub path: String,
    pub label: String,
    pub kind: String,
}

#[derive(Debug, Deserialize)]
struct ReaderSourcesResponse {
    sources: Vec<AndroidReaderSource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidReaderPage {
    pub caption: String,
    pub page_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidReaderPagesResponse {
    pub title: String,
    pub pages: Vec<AndroidReaderPage>,
}

#[derive(Debug, Deserialize)]
struct PathResponse {
    path: String,
}

#[derive(Debug, Deserialize)]
struct Base64Response {
    base64: String,
}

#[derive(Debug, Deserialize)]
struct BoolResponse {
    ok: bool,
}

pub fn init<R: Runtime>() -> TauriPlugin<R, ()> {
    Builder::<R, ()>::new("folder-picker")
        .setup(|app, api| {
            let handle = init_handle(app, api);
            app.manage(FolderPicker(handle));
            Ok(())
        })
        .build()
}

fn init_handle<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Option<tauri::plugin::PluginHandle<R>> {
    match api.register_android_plugin("com.gentleman.manager.android", "FolderPickerPlugin") {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::error!(message = %e, "初始化目錄選取器失敗，將停用讀取分類目錄功能");
            None
        }
    }
}

impl<R: Runtime> FolderPicker<R> {
    fn handle(&self) -> crate::errors::CommandResult<&tauri::plugin::PluginHandle<R>> {
        self.0.as_ref().ok_or_else(|| {
            crate::errors::CommandError::from(
                "目錄選取器不可用",
                anyhow::anyhow!("FolderPickerPlugin 尚未初始化，可能被系統或混淆移除"),
            )
        })
    }

    pub fn pick_document_tree(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickDocumentTree", ())
            .map_err(|e| crate::errors::CommandError::from("選擇目錄失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn read_text(&self, uri: &str) -> crate::errors::CommandResult<String> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<TextResponse>("readText", Payload { uri })
            .map_err(|e| crate::errors::CommandError::from("讀取檔案失敗", e))?;
        Ok(res.text)
    }

    pub fn list_subdirectory_names(
        &self,
        tree_uri: &str,
    ) -> crate::errors::CommandResult<Vec<String>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
        }
        #[derive(Deserialize)]
        struct SubdirNamesResponse {
            names: Vec<String>,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<SubdirNamesResponse>(
                "listSubdirectoryNames",
                Payload { tree_uri },
            )
            .map_err(|e| crate::errors::CommandError::from("列出下載目錄子資料夾失敗", e))?;
        Ok(res.names)
    }

    pub fn list_txt_files(
        &self,
        tree_uri: &str,
    ) -> crate::errors::CommandResult<Vec<AndroidTxtFile>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
        }
        #[derive(Deserialize)]
        struct TxtFilesResponse {
            files: Vec<AndroidTxtFile>,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<TxtFilesResponse>("listTxtFiles", Payload { tree_uri })
            .map_err(|e| crate::errors::CommandError::from("掃描 TXT 目錄失敗", e))?;
        Ok(res.files)
    }

    pub fn pick_open_zip(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickOpenZip", ())
            .map_err(|e| crate::errors::CommandError::from("選擇 ZIP 失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn pick_open_txt(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickOpenTxt", ())
            .map_err(|e| crate::errors::CommandError::from("選擇 TXT 失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn pick_open_archive(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickOpenArchive", ())
            .map_err(|e| crate::errors::CommandError::from("選擇存檔失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn append_line_to_document(
        &self,
        uri: &str,
        line: &str,
    ) -> crate::errors::CommandResult<()> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
            line: &'a str,
        }
        self.handle()?
            .run_mobile_plugin::<serde_json::Value>(
                "appendLineToDocument",
                Payload { uri, line },
            )
            .map_err(|e| crate::errors::CommandError::from("追加 TXT 行失敗", e))?;
        Ok(())
    }

    pub fn remove_line_from_document(
        &self,
        uri: &str,
        line: &str,
    ) -> crate::errors::CommandResult<bool> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
            line: &'a str,
        }
        #[derive(Deserialize)]
        struct RemovedResponse {
            removed: bool,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<RemovedResponse>(
                "removeLineFromDocument",
                Payload { uri, line },
            )
            .map_err(|e| crate::errors::CommandError::from("移除 TXT 行失敗", e))?;
        Ok(res.removed)
    }

    pub fn subdirectory_has_downloaded_content(
        &self,
        tree_uri: &str,
        subdirectory_name: &str,
    ) -> crate::errors::CommandResult<bool> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
            #[serde(rename = "subdirectoryName")]
            subdirectory_name: &'a str,
        }
        #[derive(Deserialize)]
        struct HasContentResponse {
            #[serde(rename = "hasDownloadedContent")]
            has_downloaded_content: bool,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<HasContentResponse>(
                "subdirectoryHasDownloadedContent",
                Payload {
                    tree_uri,
                    subdirectory_name,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("檢查子目錄內容失敗", e))?;
        Ok(res.has_downloaded_content)
    }

    pub fn try_remove_empty_subdirectory(
        &self,
        tree_uri: &str,
        subdirectory_name: &str,
    ) -> crate::errors::CommandResult<bool> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
            #[serde(rename = "subdirectoryName")]
            subdirectory_name: &'a str,
        }
        #[derive(Deserialize)]
        struct RemovedResponse {
            removed: bool,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<RemovedResponse>(
                "tryRemoveEmptySubdirectory",
                Payload {
                    tree_uri,
                    subdirectory_name,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("刪除空子目錄失敗", e))?;
        Ok(res.removed)
    }

    pub fn cache_document_to_file(&self, uri: &str) -> crate::errors::CommandResult<String> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<PathResponse>("cacheDocumentToFile", Payload { uri })
            .map_err(|e| crate::errors::CommandError::from("快取檔案失敗", e))?;
        Ok(res.path)
    }

    pub fn read_document_bytes(&self, uri: &str) -> crate::errors::CommandResult<Vec<u8>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<Base64Response>("readBytes", Payload { uri })
            .map_err(|e| crate::errors::CommandError::from("讀取檔案失敗", e))?;
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(res.base64)
            .map_err(|e| crate::errors::CommandError::from("解碼檔案失敗", e))
    }

    pub fn list_reader_sources(
        &self,
        tree_uri: &str,
    ) -> crate::errors::CommandResult<Vec<AndroidReaderSource>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<ReaderSourcesResponse>("listReaderSources", Payload { tree_uri })
            .map_err(|e| crate::errors::CommandError::from("掃描本地目錄失敗", e))?;
        Ok(res.sources)
    }

    pub fn load_reader_pages_from_uri(
        &self,
        uri: &str,
    ) -> crate::errors::CommandResult<AndroidReaderPagesResponse> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
        }
        self.handle()?
            .run_mobile_plugin::<AndroidReaderPagesResponse>("loadReaderPages", Payload { uri })
            .map_err(|e| crate::errors::CommandError::from("載入本地頁面失敗", e))
    }

    pub fn list_snapshot_files(
        &self,
        tree_uri: &str,
    ) -> crate::errors::CommandResult<Vec<AndroidSnapshotFile>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<SnapshotFilesResponse>(
                "listSnapshotFiles",
                Payload { tree_uri },
            )
            .map_err(|e| crate::errors::CommandError::from("掃描快照目錄失敗", e))?;
        Ok(res.files)
    }

    pub fn copy_file_to_tree(
        &self,
        tree_uri: &str,
        source_path: &str,
        relative_path: &str,
    ) -> crate::errors::CommandResult<String> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
            #[serde(rename = "sourcePath")]
            source_path: &'a str,
            #[serde(rename = "relativePath")]
            relative_path: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>(
                "copyFileToTree",
                Payload {
                    tree_uri,
                    source_path,
                    relative_path,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("複製檔案到 Android 目錄失敗", e))?;
        res.uri
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "複製檔案到 Android 目錄失敗",
                    anyhow::anyhow!("未取得目標 URI"),
                )
            })
    }

    pub fn publish_snapshot_file(
        &self,
        tree_uri: &str,
        file_name: &str,
        source_path: &str,
        cate_id: i64,
        previous_snapshot_path: Option<&str>,
    ) -> crate::errors::CommandResult<String> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
            #[serde(rename = "fileName")]
            file_name: &'a str,
            #[serde(rename = "sourcePath")]
            source_path: &'a str,
            #[serde(rename = "cateId")]
            cate_id: i64,
            #[serde(rename = "previousSnapshotPath", skip_serializing_if = "Option::is_none")]
            previous_snapshot_path: Option<&'a str>,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>(
                "publishSnapshotFile",
                Payload {
                    tree_uri,
                    file_name,
                    source_path,
                    cate_id,
                    previous_snapshot_path,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("發布快照檔失敗", e))?;
        res.uri
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "發布快照檔失敗",
                    anyhow::anyhow!("未取得目標 URI"),
                )
            })
    }

    pub fn write_snapshot_to_tree(
        &self,
        tree_uri: &str,
        file_name: &str,
        content: &str,
    ) -> crate::errors::CommandResult<String> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
            #[serde(rename = "fileName")]
            file_name: &'a str,
            content: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>(
                "writeSnapshotToTree",
                Payload {
                    tree_uri,
                    file_name,
                    content,
                },
            )
            .map_err(|e| crate::errors::CommandError::from("寫入快照檔失敗", e))?;
        res.uri
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                crate::errors::CommandError::from(
                    "寫入快照檔失敗",
                    anyhow::anyhow!("未取得目標 URI"),
                )
            })
    }

    pub fn pick_upload_document(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickUploadDocument", ())
            .map_err(|e| crate::errors::CommandError::from("選擇上傳檔案失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn pick_upload_folder(&self) -> crate::errors::CommandResult<Option<String>> {
        let res = self
            .handle()?
            .run_mobile_plugin::<UriResponse>("pickUploadFolder", ())
            .map_err(|e| crate::errors::CommandError::from("選擇上傳資料夾失敗", e))?;
        Ok(uri_pick_result(res))
    }

    pub fn list_upload_files(
        &self,
        uri: &str,
        kind: &str,
    ) -> crate::errors::CommandResult<Vec<AndroidUploadFile>> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            uri: &'a str,
            kind: &'a str,
        }
        #[derive(Deserialize)]
        struct UploadFilesResponse {
            files: Vec<AndroidUploadFile>,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<UploadFilesResponse>(
                "listUploadFiles",
                Payload { uri, kind },
            )
            .map_err(|e| crate::errors::CommandError::from("列出上傳檔案失敗", e))?;
        Ok(res.files)
    }

    pub fn probe_tree_writable(&self, tree_uri: &str) -> crate::errors::CommandResult<bool> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            #[serde(rename = "treeUri")]
            tree_uri: &'a str,
        }
        let res = self
            .handle()?
            .run_mobile_plugin::<BoolResponse>("probeTreeWritable", Payload { tree_uri })
            .map_err(|e| crate::errors::CommandError::from("測試 Android 目錄寫入權限失敗", e))?;
        Ok(res.ok)
    }
}

pub fn folder_picker<'a, R: Runtime>(
    app: &'a AppHandle<R>,
) -> crate::errors::CommandResult<State<'a, FolderPicker<R>>> {
    app.try_state::<FolderPicker<R>>().ok_or_else(|| {
        crate::errors::CommandError::from(
            "目錄選取器未初始化",
            anyhow::anyhow!("FolderPicker state missing"),
        )
    })
}
