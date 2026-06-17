Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$cargoWindows = Join-Path $PSScriptRoot "cargo-windows.ps1"
& $cargoWindows check -j 1 @args
exit $LASTEXITCODE
