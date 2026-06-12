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

if ($env:VCToolsRedistDir) {
  $candidateDirs.Add((Join-Path $env:VCToolsRedistDir "$Arch\Microsoft.VC143.CRT"))
  $candidateDirs.Add((Join-Path $env:VCToolsRedistDir "$Arch\Microsoft.VC142.CRT"))
}

$vsRoots = @(
  "${env:ProgramFiles}\Microsoft Visual Studio\2022",
  "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022",
  "${env:ProgramFiles}\Microsoft Visual Studio\2019",
  "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2019"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }

foreach ($vsRoot in $vsRoots) {
  Get-ChildItem -LiteralPath $vsRoot -Directory -ErrorAction SilentlyContinue |
    ForEach-Object {
      $redistRoot = Join-Path $_.FullName 'VC\Redist\MSVC'
      if (Test-Path -LiteralPath $redistRoot) {
        Get-ChildItem -LiteralPath $redistRoot -Directory -ErrorAction SilentlyContinue |
          Sort-Object Name -Descending |
          ForEach-Object {
            $candidateDirs.Add((Join-Path $_.FullName "$Arch\Microsoft.VC143.CRT"))
            $candidateDirs.Add((Join-Path $_.FullName "$Arch\Microsoft.VC142.CRT"))
          }
      }
    }
}

$selectedDir = $candidateDirs |
  Where-Object { $_ -and (Test-Path -LiteralPath $_) } |
  Select-Object -First 1

if (-not $selectedDir) {
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
