# Nas Manager Android — 一鍵建置簽章 APK（含進度與預估剩餘時間）
# 用法：
#   完整：build-apk.ps1 -Mode Full  /  pnpm android:build:full
#   快速：build-apk.ps1 -Mode Fast   /  pnpm android:build:fast  （僅 arm64-v8a，真機測試用）
param(
    [ValidateSet('Fast', 'Full')]
    [string]$Mode = 'Full',
    [switch]$Fast,
    [switch]$Full,
    [switch]$Quiet
)

if ($Fast) { $Mode = 'Fast' }
if ($Full) { $Mode = 'Full' }

$ErrorActionPreference = 'Stop'

$ProjectRoot = $PSScriptRoot
# 簽章 APK 一律輸出至 Nas-Manager 倉庫根目錄（非 android/ 子目錄）
$RepoRoot = Split-Path $ProjectRoot -Parent
$AndroidGradle = Join-Path $ProjectRoot 'src-tauri\gen\android'
$ApkOutDir = Join-Path $AndroidGradle 'app\build\outputs\apk\universal\release'
$UnsignedApk = Join-Path $ApkOutDir 'app-universal-release-unsigned.apk'

$IsFastBuild = ($Mode -eq 'Fast')
$TimingFile = Join-Path $ProjectRoot $(if ($IsFastBuild) { '.build-apk-timing-fast.json' } else { '.build-apk-timing.json' })

$version = '0.1.0'
$pkgJson = Join-Path $ProjectRoot 'package.json'
if (Test-Path $pkgJson) {
    $pkg = Get-Content $pkgJson -Raw | ConvertFrom-Json
    if ($pkg.version) { $version = $pkg.version }
}
$apkSuffix = if ($IsFastBuild) { '-fast' } else { '' }
$OutputApk = Join-Path $RepoRoot "Nas-Manager-Android-v$version$apkSuffix.apk"

# 快速：只編 arm64 Rust + 單 ABI，Gradle 用 daemon；完整：四 ABI + --no-daemon
$script:GradleExtraProps = @()
if ($IsFastBuild) {
    # 含 armeabi-v7a，避免 32 位元 ARM 實機載入 .so 失敗而啟動即閃退
    $script:GradleExtraProps = @(
        '-PabiList=arm64-v8a,armeabi-v7a',
        '-PtargetList=aarch64,armv7',
        '-ParchList=arm64,arm'
    )
    $script:GradleUseDaemon = $true
    $script:GradleTasksHint = 40
} else {
    $script:GradleUseDaemon = $false
    $script:GradleTasksHint = 120
}

# 各階段預估權重（秒）；若存在上次建置紀錄會覆寫 Gradle 預估
$script:PhaseBudget = @{
    Frontend = 15
    Gradle   = 300
    Sign     = 20
}
if (Test-Path $TimingFile) {
    try {
        $saved = Get-Content $TimingFile -Raw | ConvertFrom-Json
        if ($saved.frontendSeconds) { $script:PhaseBudget.Frontend = [double]$saved.frontendSeconds }
        if ($saved.gradleSeconds) { $script:PhaseBudget.Gradle = [double]$saved.gradleSeconds }
        if ($saved.signSeconds) { $script:PhaseBudget.Sign = [double]$saved.signSeconds }
    } catch { }
}
if ($IsFastBuild -and -not (Test-Path $TimingFile)) {
    $script:PhaseBudget.Gradle = 90
}
$script:TotalBudget = $script:PhaseBudget.Frontend + $script:PhaseBudget.Gradle + $script:PhaseBudget.Sign

$script:BuildStarted = Get-Date
$script:PhaseStarted = Get-Date
$script:CurrentPhase = ''
$script:GradleTasksDone = 0

function Format-Duration([TimeSpan]$ts) {
    if ($ts.TotalHours -ge 1) {
        return $ts.ToString('h\:mm\:ss')
    }
    return $ts.ToString('m\:ss')
}

function Get-OverallPercent {
    param([double]$PhaseFraction = 0)
    $phaseNames = @('Frontend', 'Gradle', 'Sign')
    $idx = [array]::IndexOf($phaseNames, $script:CurrentPhase)
    if ($idx -lt 0) { return [Math]::Min(99, [int]($PhaseFraction * 100)) }

    $doneWeight = 0.0
    for ($i = 0; $i -lt $idx; $i++) {
        $doneWeight += $script:PhaseBudget[$phaseNames[$i]]
    }
    $currentWeight = $script:PhaseBudget[$phaseNames[$idx]] * [Math]::Min(1.0, [Math]::Max(0.0, $PhaseFraction))
    $pct = (($doneWeight + $currentWeight) / $script:TotalBudget) * 100.0
    return [Math]::Min(99, [Math]::Max(0, [int]$pct))
}

function Show-BuildStatus {
    param(
        [string]$Message,
        [double]$PhaseFraction = 0
    )
    $elapsed = (Get-Date) - $script:BuildStarted
    $pct = Get-OverallPercent -PhaseFraction $PhaseFraction
    $remaining = [TimeSpan]::FromSeconds([Math]::Max(0, $script:TotalBudget - $elapsed.TotalSeconds))
    $barWidth = 28
    $filled = [Math]::Min($barWidth, [int][Math]::Round($barWidth * ($pct / 100.0)))
    $bar = ('#' * $filled).PadRight($barWidth, '-')
    $line = "[{0}] {1,3}% | 已用 {2} | 預估剩餘 ~{3} | {4}" -f $bar, $pct, (Format-Duration $elapsed), (Format-Duration $remaining), $Message
    if ($Quiet) { return }
    Write-Host "`r$line" -NoNewline
}

function Complete-BuildStatus([string]$Message) {
    if ($Quiet) {
        Write-Host $Message
        return
    }
    $elapsed = (Get-Date) - $script:BuildStarted
    $bar = ('#' * 28)
    Write-Host ("`r[{0}] 100% | 總耗時 {1} | {2}" -f $bar, (Format-Duration $elapsed), $Message)
}

function Start-Phase([string]$Name) {
    $script:CurrentPhase = $Name
    $script:PhaseStarted = Get-Date
    Show-BuildStatus "階段：$Name"
}

function End-Phase([string]$Name) {
    $sec = ((Get-Date) - $script:PhaseStarted).TotalSeconds
    $script:PhaseBudget[$Name] = [Math]::Max(5, ($script:PhaseBudget[$Name] * 0.35) + ($sec * 0.65))
    $script:TotalBudget = $script:PhaseBudget.Frontend + $script:PhaseBudget.Gradle + $script:PhaseBudget.Sign
    return [double]$sec
}

function Save-Timing($frontendSec, $gradleSec, $signSec) {
    @{
        frontendSeconds = [double]$frontendSec
        gradleSeconds   = [double]$gradleSec
        signSeconds     = [double]$signSec
        updatedAt       = (Get-Date).ToString('o')
    } | ConvertTo-Json | Set-Content -Path $TimingFile -Encoding UTF8
}

function Invoke-CommandWithProgress {
    param(
        [string]$PhaseName,
        [scriptblock]$Body
    )
    Start-Phase $PhaseName
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $timer = New-Object System.Timers.Timer
    $timer.Interval = 800
    $timer.AutoReset = $true
    $timer.Add_Elapsed({
        $budget = $script:PhaseBudget[$PhaseName]
        $frac = if ($budget -gt 0) { [Math]::Min(0.95, $sw.Elapsed.TotalSeconds / $budget) } else { 0.5 }
        Show-BuildStatus "階段：$PhaseName" -PhaseFraction $frac
    })
    $timer.Start()
    try {
        [void](& $Body)
    } finally {
        $timer.Stop()
        $timer.Dispose()
        $sw.Stop()
    }
    Show-BuildStatus "階段：$PhaseName 完成" -PhaseFraction 1
    if (-not $Quiet) { Write-Host '' }
    return (End-Phase $PhaseName)
}

function Invoke-GradleAssembleRelease {
    Push-Location $AndroidGradle
    try {
        $gradlewArgs = @('assembleRelease', '--console=plain') + $script:GradleExtraProps
        if (-not $script:GradleUseDaemon) {
            $gradlewArgs += '--no-daemon'
        }
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & .\gradlew.bat @gradlewArgs 2>&1 | ForEach-Object {
            $line = "$_"
            if ($line -match '^> Task ') {
                $script:GradleTasksDone++
                $frac = [Math]::Min(0.98, $script:GradleTasksDone / $script:GradleTasksHint)
                Show-BuildStatus "Gradle 任務 $script:GradleTasksDone …" -PhaseFraction $frac
            }
            if (-not $Quiet) {
                Write-Host $line
            }
        }
        $ErrorActionPreference = $prevEap
        if ($LASTEXITCODE -ne 0) {
            throw "gradlew assembleRelease 失敗 (exit $LASTEXITCODE)"
        }
    } finally {
        Pop-Location
    }
}

# --- 環境 ---
$javaCandidates = @(
    'C:\Program Files\Eclipse Adoptium\jdk-17.0.19.10-hotspot',
    'C:\Program Files\Eclipse Adoptium\jdk-17*',
    'C:\Program Files\Java\jdk-17*'
)
foreach ($pattern in $javaCandidates) {
    $found = Get-ChildItem -Path $pattern -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) {
        if ((Split-Path $found.FullName -Leaf).ToLower() -eq 'bin') {
            $env:JAVA_HOME = Split-Path $found.FullName -Parent
        } else {
            $env:JAVA_HOME = $found.FullName
        }
        break
    }
}
if (-not $env:JAVA_HOME) {
    throw '找不到 Java 17，請安裝 Temurin JDK 17。'
}

if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk" }
if (-not $env:NDK_HOME) { $env:NDK_HOME = Join-Path $env:ANDROID_HOME 'ndk\26.1.10909125' }
$env:Path = (Join-Path $env:JAVA_HOME 'bin') + ';' + (Join-Path $env:ANDROID_HOME 'platform-tools') + ';' + $env:Path

if (-not (Test-Path $AndroidGradle)) {
    throw 'Android 專案尚未初始化，請先執行：pnpm android:init'
}

$buildLabel = if ($IsFastBuild) { '快速測試用 APK（arm64 + armeabi-v7a）' } else { '完整 APK（四架構）' }

if (-not $Quiet) {
    Write-Host ''
    Write-Host "=== Nas Manager Android — $buildLabel ===" -ForegroundColor Cyan
    Write-Host "輸出：$OutputApk"
    if ($IsFastBuild) {
        Write-Host '說明：含 32/64 位 ARM；x86 模擬器或正式分發請用完整版。' -ForegroundColor DarkYellow
    }
    Write-Host ''
}

# 清除舊 jniLibs，避免殘留錯誤架構的 .so 被打進 APK
$jniLibsDir = Join-Path $AndroidGradle 'app\src\main\jniLibs'
if (Test-Path $jniLibsDir) {
    Remove-Item -Recurse -Force $jniLibsDir
}

# --- 1. 前端（每次必跑，確保 UI/程式修改有進 APK）---
$frontendSec = Invoke-CommandWithProgress -PhaseName 'Frontend' {
    Push-Location $ProjectRoot
    try {
        pnpm build
        if ($LASTEXITCODE -ne 0) { throw 'pnpm build 失敗' }
    } finally {
        Pop-Location
    }
}

# --- 2. Gradle + Rust / native ---
$script:GradleTasksDone = 0
$gradleSec = Invoke-CommandWithProgress -PhaseName 'Gradle' {
    Invoke-GradleAssembleRelease
}

if (-not (Test-Path $UnsignedApk)) {
    throw "找不到未簽章 APK：$UnsignedApk"
}

# --- 3. 簽章 ---
$signSec = Invoke-CommandWithProgress -PhaseName 'Sign' {
    $keystore = Join-Path $env:USERPROFILE '.android\debug.keystore'
    if (-not (Test-Path $keystore)) {
        $ksDir = Split-Path $keystore -Parent
        New-Item -ItemType Directory -Force -Path $ksDir | Out-Null
        $keytool = Join-Path $env:JAVA_HOME 'bin\keytool.exe'
        & $keytool -genkeypair -v -keystore $keystore -storepass android -alias androiddebugkey -keypass android `
            -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=Android Debug,O=Android,C=US'
    }

    $buildTools = Get-ChildItem (Join-Path $env:ANDROID_HOME 'build-tools') -Directory |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $buildTools) { throw '找不到 Android build-tools' }
    $apksigner = Join-Path $buildTools.FullName 'apksigner.bat'

    & $apksigner sign --ks $keystore --ks-pass pass:android --ks-key-alias androiddebugkey `
        --key-pass pass:android --out $OutputApk $UnsignedApk
    if ($LASTEXITCODE -ne 0) { throw 'apksigner sign 失敗' }

    & $apksigner verify --verbose $OutputApk | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'apksigner verify 失敗' }

    Get-ChildItem (Join-Path $RepoRoot 'Nas-Manager-Android-v*.apk.idsig') -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    if ($IsFastBuild) {
        $fullApk = Join-Path $RepoRoot "Nas-Manager-Android-v$version.apk"
        if ((Test-Path $fullApk) -and ($fullApk -ne $OutputApk)) {
            Write-Host "提示：完整版仍為 $fullApk" -ForegroundColor DarkGray
        }
    }
}

Save-Timing $frontendSec $gradleSec $signSec

$apkInfo = Get-Item $OutputApk
Complete-BuildStatus '建置完成'
Write-Host ''
Write-Host 'OK APK:' -ForegroundColor Green
Write-Host "  $($apkInfo.FullName)"
$apkSizeMb = [Math]::Round($apkInfo.Length / 1MB, 2)
Write-Host ('  Size ' + $apkSizeMb + ' MB | ' + $apkInfo.LastWriteTime)
Write-Host ''
Write-Host '安裝提示：建議先解除安裝舊版再安裝此檔。' -ForegroundColor Yellow
