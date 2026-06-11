Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "C:\t\verbatim" }

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
$env:CARGO_TARGET_DIR = $targetDir

if (-not $env:CMAKE_GENERATOR -and (Get-Command ninja -ErrorAction SilentlyContinue)) {
  $env:CMAKE_GENERATOR = "Ninja"
}

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
if ($env:CMAKE_GENERATOR) {
  Write-Host "CMAKE_GENERATOR=$env:CMAKE_GENERATOR"
}

cargo check --manifest-path $manifestPath -j 1
