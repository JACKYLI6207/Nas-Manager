# Download FFmpeg essentials (libzimg) for HDR tonemap; used by build-exe.ps1
param(
    [string]$OutDir = ""
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($OutDir)) {
    $OutDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'bundled\ffmpeg'
}

$ffmpegExe = Join-Path $OutDir 'ffmpeg.exe'
$ffprobeExe = Join-Path $OutDir 'ffprobe.exe'
if ((Test-Path -LiteralPath $ffmpegExe) -and (Test-Path -LiteralPath $ffprobeExe)) {
    Write-Host "FFmpeg already present: $OutDir"
    exit 0
}

$zipUrl = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
$tempZip = Join-Path $env:TEMP ('nas-ffmpeg-essentials-' + [guid]::NewGuid().ToString('N') + '.zip')
$tempExtract = Join-Path $env:TEMP ('nas-ffmpeg-extract-' + [guid]::NewGuid().ToString('N'))

Write-Host 'Downloading FFmpeg essentials...'
Invoke-WebRequest -Uri $zipUrl -OutFile $tempZip -UseBasicParsing

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Item -ItemType Directory -Force -Path $tempExtract | Out-Null

Write-Host 'Extracting...'
Expand-Archive -LiteralPath $tempZip -DestinationPath $tempExtract -Force

$binDir = Get-ChildItem -Path $tempExtract -Directory | ForEach-Object {
    Join-Path $_.FullName 'bin'
} | Where-Object { Test-Path (Join-Path $_ 'ffmpeg.exe') } | Select-Object -First 1

if (-not $binDir) {
    Remove-Item -LiteralPath $tempZip -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $tempExtract -Recurse -Force -ErrorAction SilentlyContinue
    throw 'FFmpeg zip layout unexpected (bin/ffmpeg.exe not found)'
}

Copy-Item -LiteralPath (Join-Path $binDir 'ffmpeg.exe') -Destination $ffmpegExe -Force
Copy-Item -LiteralPath (Join-Path $binDir 'ffprobe.exe') -Destination $ffprobeExe -Force

Remove-Item -LiteralPath $tempZip -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $tempExtract -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "OK FFmpeg -> $OutDir"
