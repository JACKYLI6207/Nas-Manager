# Nas Manager - build PC EXE from repo root (output in this folder)
$ErrorActionPreference = 'Stop'

$RepoRoot = $PSScriptRoot
$WindowsRoot = Join-Path $RepoRoot 'windows'
$version = '1.0.0'
$pkgJson = Join-Path $WindowsRoot 'package.json'
if (Test-Path $pkgJson) {
    $pkg = Get-Content $pkgJson -Raw | ConvertFrom-Json
    if ($pkg.version) { $version = $pkg.version }
}

$builtExe = Join-Path $WindowsRoot 'src-tauri\target\release\Nas-Manager-Windows.exe'
$outputExe = Join-Path $RepoRoot "Nas-Manager-Windows-v$version.exe"
$outputZip = Join-Path $RepoRoot "Nas-Manager-Windows-v$version.zip"

$nodejs = 'C:\Program Files\nodejs'
if (Test-Path -LiteralPath $nodejs) {
    $env:PATH = "$nodejs;" + ($env:PATH -replace '[^;]*cursor[^;]*;?', '')
}

Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue

Write-Host ''
Write-Host '=== Nas Manager Windows EXE ===' -ForegroundColor Cyan
Write-Host ('Output: ' + $outputExe)
Write-Host ''

Set-Location $WindowsRoot

Write-Host '>> pnpm build'
pnpm build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host '>> cargo build --release --features tauri/custom-protocol'
Push-Location (Join-Path $WindowsRoot 'src-tauri')
try {
    cargo build --release --bins --features tauri/custom-protocol
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

if (-not (Test-Path $builtExe)) {
    throw ('Built exe not found: ' + $builtExe)
}

$size = (Get-Item $builtExe).Length
$minBytes = 5000000
if ($size -lt $minBytes) {
    throw ('EXE too small (' + $size + ' bytes). Use tauri/custom-protocol build.')
}
$probe = python -c "import sys; d=open(sys.argv[1],'rb').read(); print('ok' if (b'index.html' in d and b'tauri://localhost' in d) else 'bad')" $builtExe
if ($LASTEXITCODE -ne 0 -or $probe -ne 'ok') {
    throw 'EXE missing embedded frontend (index.html / tauri://localhost). Rebuild with tauri/custom-protocol.'
}

Copy-Item -LiteralPath $builtExe -Destination $outputExe -Force

if (Test-Path $outputZip) { Remove-Item $outputZip -Force }
Compress-Archive -Path $outputExe -DestinationPath $outputZip -CompressionLevel Optimal

$info = Get-Item $outputExe
Write-Host ''
Write-Host 'OK EXE:' -ForegroundColor Green
Write-Host ('  ' + $info.FullName)
Write-Host ('  Size ' + [Math]::Round($info.Length / 1MB, 2) + ' MB | ' + $info.LastWriteTime)
Write-Host ('OK ZIP: ' + $outputZip)
