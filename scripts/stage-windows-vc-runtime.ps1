param(
  [ValidateSet('x64', 'arm64')]
  [string]$Arch = 'x64',

  [string]$OutputRoot = 'src-tauri\nsis\runtime'
)

$ErrorActionPreference = 'Stop'

$requiredDlls = @(
  'MSVCP140.dll',
  'MSVCP140_1.dll',
  'VCRUNTIME140.dll',
  'VCRUNTIME140_1.dll'
)

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$resolvedOutputRoot = if ([System.IO.Path]::IsPathRooted($OutputRoot)) {
  $OutputRoot
} else {
  Join-Path $repoRoot $OutputRoot
}
$outputDir = Join-Path $resolvedOutputRoot "windows-$Arch"
$runtimeConfigPath = Join-Path $resolvedOutputRoot 'tauri.windows-runtime.conf.json'

$candidateDirs = New-Object System.Collections.Generic.List[string]

function Add-RuntimeCandidateDir {
  param([string]$Path)

  if ($Path) {
    $candidateDirs.Add($Path)
  }
}

function Add-RedistVersionCandidates {
  param([string]$RedistVersionRoot)

  Add-RuntimeCandidateDir (Join-Path $RedistVersionRoot "$Arch\Microsoft.VC143.CRT")
  Add-RuntimeCandidateDir (Join-Path $RedistVersionRoot "$Arch\Microsoft.VC142.CRT")
  Add-RuntimeCandidateDir (Join-Path $RedistVersionRoot "$Arch\Microsoft.VC.CRT")
}

if ($env:VCToolsRedistDir) {
  Add-RedistVersionCandidates $env:VCToolsRedistDir
}

$vsInstallRoots = New-Object System.Collections.Generic.List[string]
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) {
  & $vswhere -all -products * -property installationPath |
    Where-Object { $_ -and (Test-Path -LiteralPath $_) } |
    ForEach-Object { $vsInstallRoots.Add($_) }
}

$vsVersionRoots = @(
  "${env:ProgramFiles}\Microsoft Visual Studio",
  "${env:ProgramFiles(x86)}\Microsoft Visual Studio"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }

foreach ($versionRoot in $vsVersionRoots) {
  Get-ChildItem -LiteralPath $versionRoot -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    ForEach-Object {
      Get-ChildItem -LiteralPath $_.FullName -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { $vsInstallRoots.Add($_.FullName) }
    }
}

$vsInstallRoots |
  Select-Object -Unique |
  ForEach-Object {
    $redistRoot = Join-Path $_ 'VC\Redist\MSVC'
    if (Test-Path -LiteralPath $redistRoot) {
      Get-ChildItem -LiteralPath $redistRoot -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Add-RedistVersionCandidates $_.FullName }
    }
  }

$runtimeRoots = @(
  "${env:SystemRoot}\System32",
  "${env:SystemRoot}\SysWOW64"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }

foreach ($runtimeRoot in $runtimeRoots) {
  $hasAllDlls = $true
  foreach ($dllName in $requiredDlls) {
    if (-not (Test-Path -LiteralPath (Join-Path $runtimeRoot $dllName))) {
      $hasAllDlls = $false
      break
    }
  }

  if ($hasAllDlls) {
    Add-RuntimeCandidateDir $runtimeRoot
  }
}

$selectedDir = $candidateDirs |
  Where-Object { $_ -and (Test-Path -LiteralPath $_) } |
  Where-Object {
    $candidate = $_
    foreach ($dllName in $requiredDlls) {
      if (-not (Test-Path -LiteralPath (Join-Path $candidate $dllName))) {
        return $false
      }
    }
    $true
  } |
  Select-Object -First 1

if (-not $selectedDir) {
  Write-Host "Checked Visual C++ runtime candidate directories:"
  $candidateDirs | Select-Object -Unique | ForEach-Object { Write-Host "  $_" }
  throw "Could not locate the Visual C++ runtime redistributable directory for $Arch. Install Visual Studio Build Tools with MSVC redistributable files."
}

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

$stagedDlls = @{}

foreach ($dllName in $requiredDlls) {
  $sourcePath = Join-Path $selectedDir $dllName
  if (-not (Test-Path -LiteralPath $sourcePath)) {
    throw "Missing required runtime DLL $dllName in $selectedDir"
  }

  $targetPath = Join-Path $outputDir $dllName
  Copy-Item -LiteralPath $sourcePath -Destination $targetPath -Force
  $stagedDlls[$dllName] = $targetPath
}

$resources = [ordered]@{
  'resources/**/*' = 'resources/'
}

foreach ($dllName in $requiredDlls) {
  $srcTauriRoot = (Resolve-Path -LiteralPath (Join-Path $repoRoot 'src-tauri')).Path.TrimEnd('\') + '\'
  $relativePath = $stagedDlls[$dllName].Substring($srcTauriRoot.Length).Replace('\', '/')
  $resources[$relativePath] = $dllName
}

$runtimeConfig = [ordered]@{
  bundle = [ordered]@{
    resources = $resources
  }
}

$runtimeConfigJson = $runtimeConfig | ConvertTo-Json -Depth 100
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($runtimeConfigPath, $runtimeConfigJson, $utf8NoBom)

if ($env:GITHUB_ENV) {
  Add-Content -LiteralPath $env:GITHUB_ENV -Value 'VERBATIM_WINDOWS_RUNTIME_CONFIG=--config src-tauri/nsis/runtime/tauri.windows-runtime.conf.json'
}

Write-Host "Staged Visual C++ runtime DLLs from $selectedDir to $outputDir"
Write-Host "Generated Tauri Windows runtime config at $runtimeConfigPath"
