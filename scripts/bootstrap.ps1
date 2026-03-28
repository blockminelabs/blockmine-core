. (Join-Path $PSScriptRoot "common.ps1")
Load-DotEnv

Write-Host "Installing JavaScript dependencies for the onchain workspace..."
Invoke-InRepo "onchain" { npm install }

Write-Host ""
Write-Host "Bootstrap complete."
Write-Host "Rust, Solana CLI, Anchor CLI, and SPL Token CLI still need to be installed separately."
