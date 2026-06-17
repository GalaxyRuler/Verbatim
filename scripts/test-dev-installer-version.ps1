Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptPath = Join-Path $PSScriptRoot "build-windows-dev-installer.ps1"
$output = & powershell -NoProfile -ExecutionPolicy Bypass -File $scriptPath -DryRun
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$devVersionLine = $output | Where-Object { $_ -like "DevVersion=*" } | Select-Object -First 1
if (-not $devVersionLine) {
  throw "Dry run did not print DevVersion."
}

$devVersion = $devVersionLine.Substring("DevVersion=".Length)
$semverWithNumericBuild = "^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*\+[0-9]+$"
if ($devVersion -notmatch $semverWithNumericBuild) {
  throw "DevVersion '$devVersion' must include numeric build metadata for Windows PE versioning."
}

Write-Host "Dev installer version OK: $devVersion"
