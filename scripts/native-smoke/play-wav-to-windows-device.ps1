[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$WavPath,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$DeviceName,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$ReceiptPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resolvedWavPath = (Resolve-Path -LiteralPath $WavPath -ErrorAction Stop).Path
$receiptParent = Split-Path -Parent $ReceiptPath
if ([string]::IsNullOrWhiteSpace($receiptParent)) {
    throw 'ReceiptPath must include a parent directory.'
}
New-Item -ItemType Directory -Force -Path $receiptParent | Out-Null

$sourcePath = Join-Path $PSScriptRoot 'windows-wasapi-playback.cs'
if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
    throw "WASAPI playback source is missing: $sourcePath"
}

Add-Type -Path $sourcePath -ErrorAction Stop
$receipt = [Verbatim.NativeSmoke.WasapiWavPlayback]::PlayMonoPcm16(
    $resolvedWavPath,
    $DeviceName
)
$json = $receipt | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText(
    $ReceiptPath,
    ($json + [Environment]::NewLine),
    [System.Text.UTF8Encoding]::new($false)
)

if (-not $receipt.success) {
    $failureClass = if ([string]::IsNullOrWhiteSpace([string]$receipt.failure_class)) {
        'unknown_failure'
    } else {
        [string]$receipt.failure_class
    }
    throw "WASAPI playback failed: $failureClass"
}

$json
