Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$cargoWindows = Join-Path $PSScriptRoot "cargo-windows.ps1"
& $cargoWindows test @args
exit $LASTEXITCODE
