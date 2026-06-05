# Nas Manager — 從倉庫根目錄建置 APK（產物輸出於本目錄）
# 用法：.\build-apk.ps1 -Mode Fast | Full
param(
    [ValidateSet('Fast', 'Full')]
    [string]$Mode = 'Fast',
    [switch]$Fast,
    [switch]$Full,
    [switch]$Quiet
)

$ErrorActionPreference = 'Stop'
$RepoRoot = $PSScriptRoot
$androidScript = Join-Path $RepoRoot 'android\build-apk.ps1'
if (-not (Test-Path $androidScript)) {
    throw "找不到 android 建置腳本：$androidScript"
}

$params = @{ Mode = $Mode }
if ($Fast) { $params.Mode = 'Fast' }
if ($Full) { $params.Mode = 'Full' }
if ($Quiet) { $params.Quiet = $true }

& $androidScript @params
exit $LASTEXITCODE
