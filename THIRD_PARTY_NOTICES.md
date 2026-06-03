# 第三方開源元件聲明（Third-Party Notices）

本文件列出 Nas Manager v1.0.0 **直接依賴**之主要開源專案及其授權。  
完整 transitive 依賴請於建置前執行：

- Android / Windows 前端：`pnpm licenses list --json`（於 `android/` 或 `windows/`）
- Rust：`cargo license --avoid-build-deps --avoid-dev-deps`（於各 `src-tauri/`）

以下授權摘要僅供參考；**以各專案官方 LICENSE 全文為準**。

---

## 1. 本專案授權

| 專案 | 授權 |
|------|------|
| Nas Manager 原始碼 | [MIT](./LICENSE) |

---

## 2. 前端（Vue / TypeScript）

| 元件 | 版本（約） | 授權 | 用途 |
|------|------------|------|------|
| [Vue.js](https://github.com/vuejs/core) | 3.5.x | MIT | UI 框架 |
| [Vite](https://github.com/vitejs/vite) | 6.x | MIT | 建置工具 |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | 2.x | Apache-2.0 **或** MIT | 桌面/行動殼層 JS API |
| [@tauri-apps/plugin-dialog](https://github.com/tauri-apps/plugins-workspace) | 2.x | Apache-2.0 **或** MIT | 系統對話框 |
| [@tauri-apps/plugin-opener](https://github.com/tauri-apps/plugins-workspace) | 2.x | Apache-2.0 **或** MIT | 開啟外部連結 |
| [opencc-js](https://github.com/nk2028/opencc-js) | 1.3.x | Apache-2.0 | 繁簡轉換 |
| [TypeScript](https://github.com/microsoft/TypeScript) | 5.6.x | Apache-2.0 | 型別系統 |

### Windows 前端額外依賴

| 元件 | 版本（約） | 授權 | 用途 |
|------|------------|------|------|
| [Naive UI](https://github.com/tusen-ai/naive-ui) | 2.43.x | MIT | UI 元件 |
| [Pinia](https://github.com/vuejs/pinia) | 3.x | MIT | 狀態管理 |
| [@phosphor-icons/vue](https://github.com/phosphor-icons/vue) | 2.x | MIT | 圖示 |
| [UnoCSS](https://github.com/unocss/unocss) | 66.x | MIT | 原子化 CSS |

---

## 3. Rust 後端（Tauri）

| 元件 | 版本（約） | 授權 | 用途 |
|------|------------|------|------|
| [Tauri](https://github.com/tauri-apps/tauri) | 2.x | Apache-2.0 **或** MIT | 應用殼層 |
| [tauri-specta / specta](https://github.com/oscartbeaumont/specta) | 2.x RC | MIT | 型別綁定 |
| [Tokio](https://github.com/tokio-rs/tokio) | 1.x | MIT | 非同步 runtime |
| [Serde](https://github.com/serde-rs/serde) | 1.x | Apache-2.0 **或** MIT | 序列化 |
| [reqwest](https://github.com/seanmonstar/reqwest) | 0.12.x | Apache-2.0 **或** MIT | HTTP 客戶端 |
| [Axum](https://github.com/tokio-rs/axum) | 0.8.x | MIT | Windows 遠端 HTTP 伺服器 |
| [scraper](https://github.com/causal-agent/scraper) | 0.23.x | MIT | HTML 解析（工具函式） |
| [image](https://github.com/image-rs/image) | 0.25.x | Apache-2.0 **或** MIT | 圖片處理 |
| [zip](https://github.com/zip-rs/zip2) | 2.x | MIT | ZIP/CBZ 讀取 |
| [mdns-sd](https://github.com/kevinmehall/rust-mdns) | 0.13.x | MIT | 區網服務探索 |
| [notify](https://github.com/notify-rs/notify) | 8.x | CC0-1.0 **或** MIT | 檔案監看 |
| [anyhow](https://github.com/dtolnay/anyhow) | 1.x | Apache-2.0 **或** MIT | 錯誤處理 |
| [tracing](https://github.com/tokio-rs/tracing) | 0.1.x | MIT | 日誌 |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12.x | Apache-2.0 **or** MIT | 同步原語 |
| [rayon](https://github.com/rayon-rs/rayon) | 1.x | Apache-2.0 **or** MIT | 平行計算 |
| [uuid](https://github.com/uuid-rs/uuid) | 1.x | Apache-2.0 **or** MIT | UUID |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.22.x | Apache-2.0 **or** MIT | Base64 |
| [regex](https://github.com/rust-lang/regex) | 1.x | Apache-2.0 **or** MIT | 正則 |
| [yaserde](https://github.com/media-io/yaserde) | 0.12.x | MIT | XML |
| [natord](https://github.com/wookayin/natord) | 1.x | MIT | 自然排序 |

> Rust 生態系尚有大量 transitive 依賴（多為 MIT / Apache-2.0 / BSD-3-Clause）。發佈前請以 `cargo license` 產生 SPDX 清單並附於 Release 資產（若您的法務流程要求）。

---

## 4. Android 原生（Gradle）

| 元件 | 授權 | 用途 |
|------|------|------|
| [AndroidX / Material Components](https://github.com/androidx/androidx) | Apache-2.0 | UI 與相容函式庫 |
| [Google ExoPlayer / Media3](https://github.com/androidx/media) | Apache-2.0 | 本地/HTTP Range 串流影片播放核心 |
| [Jellyfin media3-ffmpeg-decoder](https://github.com/jellyfin/jellyfin-androidx-media) | MIT（請以發佈版 POM/AAR 所附 LICENSE 為準） | Media3 FFmpeg 軟體解碼擴充 |
| [Kotlin / Gradle](https://kotlinlang.org/) | Apache-2.0 | 建置與語言 |

### 4.1 Just Player（moneytoo/Player）— 技術參考

| 項目 | 說明 |
|------|------|
| **專案** | https://github.com/moneytoo/Player |
| **常見名稱** | Just Player |
| **授權** | [Unlicense](https://unlicense.org/)（公有領域） |
| **與 Nas Manager 之關係** | 開發時**參考** ExoPlayer/Media3 整合模式與播放器互動設計；**未**整包合併 Just Player App，亦未修改後以 Just Player 名義發佈 |
| **實作** | 本儲存庫 `android/.../LocalVideoPlayerActivity.kt` 等檔案為 Nas Manager **獨立實作與維護** |
| **義務** | Unlicense 不要求著作權標示，但本專案基於透明原則仍於此列出；再分發時請一併保留本文件 |

> FFmpeg 為多媒體解碼元件，可能涉及專利議題；商業使用或再分發前請自行評估合規需求。

完整 Gradle 依賴樹：於 `android/src-tauri/gen/android` 執行 `./gradlew :app:dependencies`（需先完成 Android 專案初始化）。

---

## 5. 授權全文取得方式

| 授權 | 官方連結 |
|------|----------|
| MIT | https://opensource.org/licenses/MIT |
| Apache-2.0 | https://www.apache.org/licenses/LICENSE-2.0 |
| BSD-3-Clause | https://opensource.org/licenses/BSD-3-Clause |
| CC0-1.0 | https://creativecommons.org/publicdomain/zero/1.0/ |

---

## 6. 聯絡

若您認為本專案錯誤標示某元件之授權，請於 GitHub Issues 提出，我們將盡速更正。

**本文件為法律資訊之整理，不構成法律意見。**
