# Nas Manager v2.1.0

## 附件

| 產物 | 版本 |
|------|------|
| Windows EXE / ZIP | **2.1.0** |
| Android 完整 APK / ZIP | **2.0.0** |

## 重點更新

### Windows（2.1.0）

- 區網上傳優先改為**目的端拉取**（與下載同級大緩衝），大幅改善跨電腦上傳卡頓／降速。
- 來源機須**遠端管理啟用且執行中**；兩邊皆建議更新至本版。
- 若目的端過舊或本機無法當來源，自動退回推送上傳。
- `remote_api` = **14**（`pull-upload`、`/api/v1/xfer-source/download`）。

### Windows（2.0.0 起）

- 上傳推送路徑調教：4 MiB 緩衝、Content-Length、伺服端 BufWriter。

### Android（2.0.0）

- 遠端上傳客戶端調教：4 MiB 緩衝、Content-Length、進度不擋傳輸。
- 識別不變：可與 Gentleman Manager **並存安裝**（`applicationId` = `com.nas.manager.android`）。

## 使用提醒

- 跨 PC 上傳滿速：兩台皆裝 **Windows 2.1.0**，且上傳來源那台遠端管理保持執行。
- Android 上傳至 PC：建議 PC ≥ 2.0.0。
- 舊版 Release（如 v1.3.0）仍可下載；本版為**新建**標籤，未覆蓋舊附件。
