## Nas Manager v1.3.0

### 新功能
- **跨 PC 剪貼簿貼上**：在 A 台 PC 剪下／複製後，可在 B 台 PC 貼上；實際傳輸為 PC↔PC（客戶端只下指令）
- **傳輸診斷日誌**：貼上／跨 PC 貼上進行中，overlay 即時顯示 health、列檔、下載等伺服端診斷

### 修正
- Windows EXE 視窗標題版號與 `package.json` 同步（不再卡在舊版 1.2.x）
- 跨 PC 貼上取消與 health 檢查可更快中斷

### PC 客戶端更新（v1.3.1，2026-08）
- **串流收藏**：修正從收藏點播影片時開啟兩個外部播放器的問題
- **遠端管理**：「操作」選單新增「收藏此漫畫／收藏此影片」（對齊 Android）
- **對話框**：串流收藏刪除／改名、清空串流列表、PC 取名等，改為 App 內浮層或 Tauri 原生對話框（修正 WebView2 `window.confirm` 未確認即執行的問題）

### Android 更新（v1.3.2，2026-08）
- **影片播放**：雙指放大後，點擊畫面可正常開關控制列 UI
- **快轉／倒退**：統一為 **30 秒**；長按可連續快轉／倒退
- **進度條**：拖曳未放開時即時 seek 預覽畫面（串流時仍受關鍵幀間隔影響）

### 安裝
- **PC**：請下載 **`Nas-Manager-Windows-v1.3.1.exe`**（或 `.zip`）；兩台要互傳檔案的電腦都需安裝並重開遠端管理
- **Android**：請下載 **`Nas-Manager-Android-v1.3.2.apk`**（或 `.zip`）

### 附件
- `Nas-Manager-Windows-v1.3.1.exe` / `.zip`（PC 客戶端；Release 標籤仍為 **v1.3.0**）
- `Nas-Manager-Android-v1.3.2.apk` / `.zip`
