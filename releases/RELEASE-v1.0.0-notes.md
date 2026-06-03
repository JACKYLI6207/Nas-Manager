## Nas Manager v1.0.0

區網 NAS 管理工具正式版：**Android 客戶端** + **Windows PC 配套端**。

### 下載檔案

| 檔案 | 說明 |
|------|------|
| `Nas-Manager-Android-v1.0.0.apk` | Android 正式版（arm / arm64 / x86 / x86_64） |
| `Nas-Manager-Windows-v1.0.0.exe` | Windows PC 遠端管理配套端 |

### 功能概覽

**Android**
- 漫畫閱讀：本地 ZIP/CBZ/資料夾；PC 串流閱讀（需配套 EXE，`remote_api ≥ 8`）
- 影片播放：本地檔案；PC HTTP Range 串流（Media3 / ExoPlayer）
- 遠端管理：mDNS/UDP 探索 PC、瀏覽分享目錄、上傳/下載/檔案操作

**Windows**
- 多分享根目錄、HTTP 遠端 API（瀏覽/下載/上傳/串流/漫畫分頁）
- 區網服務探索（mDNS + UDP）

### 快速開始

1. 在 PC 執行 **Windows EXE** → 設定分享資料夾 → 啟用遠端管理  
2. 在 Android 安裝 **APK** → 開啟「遠端管理」→ 掃描/連線 PC  
3. 即可串流漫畫/影片，或進行檔案傳輸  

### 重要提醒

- 本軟體**不提供**任何第三方內容；僅存取**您自行指定**的分享目錄。  
- 請僅在**可信任之區域網路**使用，並妥善保管遠端管理 Token。  
- Android 影片播放參考 [Just Player (moneytoo/Player)](https://github.com/moneytoo/Player) 之 Media3 整合方式；詳見儲存庫內 `DISCLAIMER.md` 與 `THIRD_PARTY_NOTICES.md`。  
- 使用前請閱讀儲存庫根目錄 **[DISCLAIMER.md](./DISCLAIMER.md)**（免責聲明）及 **[THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)**（第三方授權）。

### 原始碼

本 Release 對應 Git 標籤 `v1.0.0` 之原始碼；建置方式見 `docs/BUILD.md`。
