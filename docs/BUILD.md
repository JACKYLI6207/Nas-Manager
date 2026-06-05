# 建置指南

## 環境需求

- Node.js 20+、pnpm 9+
- Rust stable（含 Android target 若建置 APK）
- Android SDK / NDK（APK）
- Visual Studio Build Tools（Windows EXE）

## Android APK

```powershell
cd Nas-Manager
pnpm install --dir android
.\build-apk.ps1 -Mode Full    # 正式四架構
.\build-apk.ps1 -Mode Fast    # 真機快速測試
```

輸出（倉庫根目錄）：`Nas-Manager-Android-v1.0.0.apk` / `Nas-Manager-Android-v1.0.0-fast.apk`

## Windows EXE

```powershell
cd Nas-Manager
pnpm install --dir windows
.\build-exe.ps1
```

輸出（倉庫根目錄）：`Nas-Manager-Windows-v1.0.0.exe`、`Nas-Manager-Windows-v1.0.0.zip`

## 注意

- 首次 Android 建置需 `pnpm tauri android init`（若 `gen/android` 不完整）。
- Kotlin 插件位於 `android/src-tauri/gen/android/`，修改後須重新建置 APK。
- PC 端 `remote_api` 版本見 `windows/src-tauri/src/remote_management.rs`（v1.0.0 = 8）。
