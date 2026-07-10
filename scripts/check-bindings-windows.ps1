Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"
$testManifestPath = Join-Path $repoRoot "src-tauri\windows\test-common-controls.manifest"
$targetDir = if ($env:CARGO_TARGET_DIR) {
  $env:CARGO_TARGET_DIR
} else {
  "C:\t\verbatim"
}

if (-not (Test-Path -LiteralPath $testManifestPath)) {
  throw "Windows test manifest is missing: $testManifestPath"
}

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

Write-Host "Building ignored TypeScript bindings exporter"
$cargoMessages = @(
  & cargo test --manifest-path $manifestPath --test export_bindings --no-run --message-format=json
)
$cargoExitCode = $LASTEXITCODE

if ($cargoExitCode -ne 0) {
  foreach ($line in $cargoMessages) {
    try {
      $message = $line | ConvertFrom-Json
      if ($message.reason -eq "compiler-message" -and $message.message.rendered) {
        Write-Host $message.message.rendered.TrimEnd()
      }
    } catch {
      Write-Host $line
    }
  }
  exit $cargoExitCode
}

$exporterExecutables = @(
  @(
    foreach ($line in $cargoMessages) {
      try {
        $message = $line | ConvertFrom-Json
      } catch {
        continue
      }

      if (
        $message.reason -eq "compiler-artifact" -and
        $message.target.name -eq "export_bindings" -and
        $message.executable
      ) {
        $message.executable
      }
    }
  ) | Select-Object -Unique
)

if ($exporterExecutables.Count -ne 1) {
  throw "Expected one bindings exporter executable, found $($exporterExecutables.Count)."
}

$manifestTool = Get-Command mt.exe -ErrorAction SilentlyContinue
if ($null -eq $manifestTool) {
  $windowsKitsRoot = Join-Path ([Environment]::GetFolderPath("ProgramFilesX86")) "Windows Kits\10\bin"
  $manifestTool = @(
    Get-ChildItem -LiteralPath $windowsKitsRoot -Directory -ErrorAction SilentlyContinue |
      Sort-Object Name -Descending |
      ForEach-Object {
        $candidate = Join-Path $_.FullName "x64\mt.exe"
        if (Test-Path -LiteralPath $candidate) {
          $candidate
        }
      }
  ) | Select-Object -First 1
}

if ($null -eq $manifestTool) {
  throw "mt.exe is required to apply the Windows test manifest to the bindings exporter."
}

$manifestToolPath = if ($manifestTool -is [string]) {
  $manifestTool
} else {
  $manifestTool.Source
}
$exporterExecutable = $exporterExecutables[0]

Write-Host "Applying Windows test manifest to bindings exporter"
& $manifestToolPath "-manifest" $testManifestPath "-outputresource:$exporterExecutable;#1"
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

Push-Location -LiteralPath (Join-Path $repoRoot "src-tauri")
try {
  & $exporterExecutable "--ignored"
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
} finally {
  Pop-Location
}

& git -C $repoRoot diff --exit-code -- src/bindings.ts
exit $LASTEXITCODE
