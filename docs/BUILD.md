# 建置指南

## 環境需求

- Node.js 20+、pnpm 9+
- Rust stable（含 Android target 若建置 APK）
- Android SDK / NDK（APK）
- Visual Studio Build Tools（Windows EXE）

## Android APK

```powershell
cd android
pnpm install
.\build-apk.ps1 -Mode Full    # 正式四架構
.\build-apk.ps1 -Mode Fast    # 真機快速測試
```

輸出：`Nas-Manager-Android-v1.0.0.apk`

## Windows EXE

```powershell
cd windows
pnpm install
pnpm build
pnpm tauri build
```

輸出：`src-tauri/target/release/Nas-Manager-Windows.exe`（依 `tauri.conf.json` 命名）

## 注意

- 首次 Android 建置需 `pnpm tauri android init`（若 `gen/android` 不完整）。
- Kotlin 插件位於 `android/src-tauri/gen/android/`，修改後須重新建置 APK。
- PC 端 `remote_api` 版本見 `windows/src-tauri/src/remote_management.rs`（v1.0.0 = 8）。
