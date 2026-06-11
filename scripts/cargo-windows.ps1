param(
  [Parameter(Mandatory = $true, Position = 0)]
  [ValidateSet("build", "check", "test")]
  [string]$Command,

  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$CargoArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "C:\t\verbatim" }

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
$env:CARGO_TARGET_DIR = $targetDir

if (-not $env:TrackFileAccess) {
  $env:TrackFileAccess = "false"
}

if (-not $env:CMAKE_GENERATOR) {
  if (Get-Command ninja -ErrorAction SilentlyContinue) {
    $env:CMAKE_GENERATOR = "Ninja"
  } else {
    throw "Ninja is required for Windows native builds. Install Ninja or put ninja.exe on PATH."
  }
}

if ($env:CMAKE_GENERATOR -ne "Ninja") {
  Write-Warning "CMAKE_GENERATOR=$env:CMAKE_GENERATOR; Verbatim's Windows native build is verified with Ninja."
}

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "CMAKE_GENERATOR=$env:CMAKE_GENERATOR"
Write-Host "TrackFileAccess=$env:TrackFileAccess"

& cargo $Command --manifest-path $manifestPath @CargoArgs
