. (Join-Path $PSScriptRoot "common.ps1")
Load-DotEnv
Assert-Command "anchor"

Invoke-InRepo "onchain" { anchor build }

