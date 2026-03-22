$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$legacyScript = Join-Path $repoRoot "scripts\build-miner-exe.ps1"

if (-not (Test-Path $legacyScript)) {
    throw "Missing build script: $legacyScript"
}

powershell -ExecutionPolicy Bypass -File $legacyScript
