$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$targetTriple = "x86_64-pc-windows-gnu"
$wslScript = "/mnt/c/Users/drums/Desktop/BLOC/miner-client/scripts/wsl-build-windows.sh"
$exeSource = Join-Path $root "miner-client\target\$targetTriple\release\blockmine-miner.exe"
$distDir = Join-Path $root "dist"
$exeTarget = Join-Path $distDir "blockmine-miner.exe"
$launcherSource = Join-Path $root "scripts\start-miner-devnet.bat"
$launcherTarget = Join-Path $distDir "start-miner-devnet.bat"
$readmeSource = Join-Path $root "scripts\README-miner-exe.txt"
$readmeTarget = Join-Path $distDir "README-miner-exe.txt"

if (-not (Test-Path $distDir)) {
    New-Item -ItemType Directory -Path $distDir | Out-Null
}

wsl bash -lc "bash $wslScript --features opencl"

if (-not (Test-Path $exeSource)) {
    throw "Windows executable not found at $exeSource"
}

Copy-Item -Path $exeSource -Destination $exeTarget -Force
Copy-Item -Path $launcherSource -Destination $launcherTarget -Force
Copy-Item -Path $readmeSource -Destination $readmeTarget -Force
Write-Host "Created $exeTarget"
