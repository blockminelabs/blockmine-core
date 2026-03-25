$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$targetTriple = "x86_64-pc-windows-gnu"
$rootWsl = $root -replace "\\", "/"
$driveLetter = $rootWsl.Substring(0, 1).ToLowerInvariant()
$rootWslPath = "/mnt/$driveLetter$($rootWsl.Substring(2))"
$wslScript = "$rootWslPath/miner-client/scripts/wsl-build-windows.sh"
$exeSource = Join-Path $root "miner-client\target\$targetTriple\release\blockmine-studio.exe"
$distDir = Join-Path $root "dist"
$exeTarget = Join-Path $distDir "Blockmine Miner.exe"
$launcherTarget = Join-Path $distDir "start-blockmine-studio.bat"
$readmeSource = Join-Path $root "scripts\README-miner-exe.txt"
$readmeTarget = Join-Path $distDir "README-blockmine-studio.txt"

if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir | Out-Null
}

wsl bash -lc "bash '$wslScript' --features opencl --bin blockmine-studio"

if (-not (Test-Path $exeSource)) {
    throw "Windows executable not found at $exeSource"
}

Copy-Item -Path $exeSource -Destination $exeTarget -Force
Copy-Item -Path $readmeSource -Destination $readmeTarget -Force
@'
@echo off
cd /d %~dp0
start "" "%~dp0Blockmine Miner.exe"
'@ | Set-Content -Path $launcherTarget -NoNewline
Write-Host "Created $exeTarget"
