# Nas Manager

區網 NAS 管理工具：**Android 客戶端** + **Windows PC 配套端**。在區域網路內瀏覽、傳輸、串流播放您**自行擁有**的檔案（漫畫 ZIP/CBZ、影片等）。

> 本專案為開源版本，僅包含 v1.0.0 正式發佈之功能範圍。請先閱讀 [DISCLAIMER.md](./DISCLAIMER.md) 與 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

## 功能概覽

### Android（`android/`）

| 模組 | 說明 |
|------|------|
| 漫畫閱讀 | 本地 ZIP/資料夾；PC 串流閱讀（需配套 EXE，`remote_api ≥ 8`） |
| 影片播放 | 本地檔案；PC HTTP Range 串流（ExoPlayer） |
| 遠端管理 | mDNS/UDP 探索 PC、瀏覽分享目錄、上傳/下載/檔案操作 |

### Windows（`windows/`）

| 模組 | 說明 |
|------|------|
| 遠端管理服務 | 多分享根目錄、HTTP API（瀏覽/下載/上傳/串流/漫畫分頁）、區網探索 |

## 正式版 v1.0.0

預編譯檔案見本儲存庫 **Releases** 頁面（標籤 `v1.0.0`；本機副本在 `releases/`）：

| 檔案 | 說明 |
|------|------|
| `Nas-Manager-Android-v1.0.0.apk` | Android 正式版（四架構） |
| `Nas-Manager-Windows-v1.0.0.exe` | Windows PC 配套端 |

1. 在 PC 執行 EXE → 設定分享資料夾 → 啟用遠端管理  
2. 在 Android 安裝 APK →「遠端管理」掃描/連線 PC → 漫畫/影片串流或檔案傳輸  

## 從原始碼建置

詳見 [docs/BUILD.md](./docs/BUILD.md)。

```text
Nas-Manager/
├── android/     # Tauri 2 + Vue 3 Android
├── windows/     # Tauri 2 + Vue 3 Windows（僅遠端管理 UI）
├── releases/    # 正式版 APK/EXE（供 Release 上傳）
└── docs/
```

## 授權

本專案以 [MIT License](./LICENSE) 釋出。第三方元件授權見 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

## 免責

使用本軟體即表示您同意 [DISCLAIMER.md](./DISCLAIMER.md) 之全部條款。
