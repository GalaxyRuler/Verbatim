param(
  [string]$DevVersion,
  [string]$TargetDir,
  [string]$OutputDir,
  [string]$Bundles = "nsis",
  [switch]$Sign,
  [switch]$DryRun,

  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $repoRoot "package.json"

function Get-DefaultDevVersion {
  param([string]$StableVersion)

  $parts = $StableVersion.Split(".")
  if ($parts.Count -ne 3) {
    throw "Stable package version '$StableVersion' is not MAJOR.MINOR.PATCH."
  }

  $major = [int]$parts[0]
  $minor = [int]$parts[1]
  $patch = [int]$parts[2] + 1
  $now = Get-Date
  $stamp = $now.ToString("yyyyMMddHHmm")
  $minuteOfDay = ($now.Hour * 60) + $now.Minute
  $tenMinuteSlot = [math]::Floor($minuteOfDay / 10)
  $windowsBuildNumber = (($now.DayOfYear - 1) * 144) + $tenMinuteSlot
  return "$major.$minor.$patch-dev.$stamp+$windowsBuildNumber"
}

function Assert-DevSemVer {
  param([string]$Version)

  $semverPattern = "^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)(\+[0-9]+)$"
  if ($Version -notmatch $semverPattern) {
    throw "DevVersion '$Version' must be valid SemVer with prerelease and numeric build metadata, for example 0.8.8-dev.1+42."
  }

  if ($Version -notmatch "-") {
    throw "DevVersion '$Version' must be a prerelease version, for example 0.8.8-dev.1+42."
  }

  $buildMetadata = $Version -replace "^.*\+", ""
  $buildNumber = [int]$buildMetadata
  if ($buildNumber -lt 0 -or $buildNumber -gt 65535) {
    throw "DevVersion '$Version' build metadata must fit in a Windows PE version word (0-65535)."
  }
}

$packageJson = Get-Content -LiteralPath $packageJsonPath -Raw | ConvertFrom-Json
$stableVersion = [string]$packageJson.version
if (-not $DevVersion) {
  $DevVersion = Get-DefaultDevVersion -StableVersion $stableVersion
}
Assert-DevSemVer -Version $DevVersion

$targetDirValue = if ($TargetDir) {
  $TargetDir
} elseif ($env:VERBATIM_DEV_TARGET_DIR) {
  $env:VERBATIM_DEV_TARGET_DIR
} else {
  "C:\b-verbatim-dev"
}

$outputDirValue = if ($OutputDir) {
  $OutputDir
} else {
  Join-Path $repoRoot ".local-builds\dev"
}

$resolvedTargetDir = [System.IO.Path]::GetFullPath($targetDirValue)
$resolvedOutputDir = [System.IO.Path]::GetFullPath($outputDirValue)
New-Item -ItemType Directory -Force -Path $resolvedTargetDir | Out-Null
New-Item -ItemType Directory -Force -Path $resolvedOutputDir | Out-Null

$devConfigPath = Join-Path $resolvedOutputDir "tauri.dev.generated.json"
$generateConfigScript = Join-Path $repoRoot "scripts\generate-dev-tauri-config.ts"
& bun $generateConfigScript --version $DevVersion --output $devConfigPath
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$bundleRoot = Join-Path $resolvedTargetDir "release\bundle"
$nsisBundleDir = Join-Path $bundleRoot "nsis"
if (Test-Path -LiteralPath $nsisBundleDir) {
  $resolvedNsisBundleDir = [System.IO.Path]::GetFullPath($nsisBundleDir)
  if (-not $resolvedNsisBundleDir.StartsWith($resolvedTargetDir, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove bundle directory outside target dir: $resolvedNsisBundleDir"
  }
  Remove-Item -LiteralPath $resolvedNsisBundleDir -Recurse -Force
}

$env:CARGO_TARGET_DIR = $resolvedTargetDir
$env:VERBATIM_DEV_VERSION = $DevVersion
$env:VITE_VERBATIM_DEV_VERSION = $DevVersion
if (-not $env:CMAKE_GENERATOR) {
  $env:CMAKE_GENERATOR = "Ninja"
}
if (-not $env:TrackFileAccess) {
  $env:TrackFileAccess = "false"
}

$buildArgs = @(
  "run",
  "tauri",
  "build",
  "--bundles",
  $Bundles,
  "--config",
  $devConfigPath,
  "--ignore-version-mismatches"
)

if (-not $Sign) {
  $buildArgs += "--no-sign"
}
if ($TauriArgs) {
  $buildArgs += $TauriArgs
}

Write-Host "ProductName=Verbatim Dev"
Write-Host "Identifier=com.galaxyruler.verbatim.dev"
Write-Host "MainBinaryName=verbatim-dev"
Write-Host "DevVersion=$DevVersion"
Write-Host "VERBATIM_DEV_VERSION=$env:VERBATIM_DEV_VERSION"
Write-Host "VITE_VERBATIM_DEV_VERSION=$env:VITE_VERBATIM_DEV_VERSION"
Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "CMAKE_GENERATOR=$env:CMAKE_GENERATOR"
Write-Host "TrackFileAccess=$env:TrackFileAccess"
Write-Host "OutputDir=$resolvedOutputDir"
Write-Host "DevConfig=$devConfigPath"
Write-Host ("Command=bun {0}" -f ($buildArgs -join " "))

if ($DryRun) {
  exit 0
}

Push-Location $repoRoot
try {
  $buildStartedAt = Get-Date
  & bun @buildArgs
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }

  if (-not (Test-Path -LiteralPath $bundleRoot)) {
    throw "Bundle output was not created: $bundleRoot"
  }

  $artifacts = Get-ChildItem -LiteralPath $bundleRoot -Recurse -File |
    Where-Object {
      $_.LastWriteTime -ge $buildStartedAt -and
      $_.Extension -in @(".exe", ".msi", ".zip", ".dmg", ".deb", ".rpm", ".AppImage")
    } |
    Sort-Object LastWriteTime -Descending

  if (-not $artifacts) {
    throw "No installer artifacts were produced under $bundleRoot"
  }

  foreach ($artifact in $artifacts) {
    Copy-Item -LiteralPath $artifact.FullName -Destination $resolvedOutputDir -Force
  }

  $artifacts |
    Select-Object FullName, Length, LastWriteTime |
    Format-Table -AutoSize

  Write-Host "Copied dev installer artifacts to $resolvedOutputDir"
} finally {
  Pop-Location
}
