param(
  [string]$TargetDir,
  [string]$Bundles = "nsis",

  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetDirValue = if ($TargetDir) {
  $TargetDir
} elseif ($env:CARGO_TARGET_DIR) {
  $env:CARGO_TARGET_DIR
} elseif ($env:VERBATIM_WINDOWS_TARGET_DIR) {
  $env:VERBATIM_WINDOWS_TARGET_DIR
} else {
  "C:\b"
}

$resolvedTargetDir = [System.IO.Path]::GetFullPath($targetDirValue)
New-Item -ItemType Directory -Force -Path $resolvedTargetDir | Out-Null

$env:CARGO_TARGET_DIR = $resolvedTargetDir
if (-not $env:TrackFileAccess) {
  $env:TrackFileAccess = "false"
}

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "TrackFileAccess=$env:TrackFileAccess"
Write-Host "Bundles=$Bundles"

Push-Location $repoRoot
try {
  & bun run tauri build -- --bundles $Bundles @TauriArgs
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }

  $bundleRoot = Join-Path $resolvedTargetDir "release\bundle"
  if (Test-Path -LiteralPath $bundleRoot) {
    Get-ChildItem -LiteralPath $bundleRoot -Recurse -File |
      Sort-Object LastWriteTime -Descending |
      Select-Object -First 10 FullName, Length, LastWriteTime |
      Format-Table -AutoSize
  }
} finally {
  Pop-Location
}
