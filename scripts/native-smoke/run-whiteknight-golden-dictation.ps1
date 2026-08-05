[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$AppPath,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$ModelDirectory,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$WavPath,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$ModelId,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$InputDeviceName,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$OutputDeviceName,

    [string]$WhiteKnightName = 'whiteknight',

    [string]$RemoteModulePath = (Join-Path $env:USERPROFILE '.codex-remote-functions\CodexRemoteFunctions\CodexRemoteFunctions.psm1'),

    [string]$RemoteConfigPath = (Join-Path $env:USERPROFILE '.codex-remote-functions\config\remotes.json'),

    [string]$ArtifactRoot = 'C:\CodexScratch\verbatim-golden-dictation',

    [ValidateRange(60, 900)]
    [int]$TaskTimeoutSeconds = 540,

    [ValidateRange(1, 60)]
    [int]$ConnectTimeoutSeconds = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Resolve-RequiredPath {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue,

        [Parameter(Mandatory)]
        [string]$Label,

        [switch]$Directory
    )

    if (-not [System.IO.Path]::IsPathRooted($PathValue)) {
        throw "$Label must be an absolute path."
    }
    if (-not (Test-Path -LiteralPath $PathValue)) {
        throw "$Label does not exist: $PathValue"
    }
    if ($Directory -and -not (Test-Path -LiteralPath $PathValue -PathType Container)) {
        throw "$Label must be a directory: $PathValue"
    }
    if (-not $Directory -and -not (Test-Path -LiteralPath $PathValue -PathType Leaf)) {
        throw "$Label must be a file: $PathValue"
    }
    return (Resolve-Path -LiteralPath $PathValue).Path
}

function Resolve-OutputRoot {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue
    )

    if (-not [System.IO.Path]::IsPathRooted($PathValue)) {
        throw 'ArtifactRoot must be an absolute path.'
    }
    return [System.IO.Path]::GetFullPath($PathValue)
}

function ConvertTo-WindowsScpPath {
    param(
        [Parameter(Mandatory)]
        [string]$WindowsPath
    )

    if ($WindowsPath -notmatch '^[A-Za-z]:\\') {
        throw "Expected an absolute Windows path, got: $WindowsPath"
    }
    $driveName = $WindowsPath.Substring(0, 1)
    $pathPart = $WindowsPath.Substring(3).Replace('\', '/')
    return "/${driveName}:/$pathPart"
}

function ConvertTo-PowerShellLiteral {
    param(
        [AllowEmptyString()]
        [string]$Value
    )

    return "'" + $Value.Replace("'", "''") + "'"
}

function ConvertTo-ScpTarget {
    param(
        [Parameter(Mandatory)]
        $Remote
    )

    if (-not [string]::IsNullOrWhiteSpace([string]$Remote.SshAlias)) {
        return [string]$Remote.SshAlias
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Remote.TailscaleIP)) {
        return ('{0}@{1}' -f $Remote.User, $Remote.TailscaleIP)
    }
    return ('{0}@{1}' -f $Remote.User, $Remote.HostName)
}

function Invoke-Scp {
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments,

        [Parameter(Mandatory)]
        [string]$Description
    )

    $scpOutput = & scp @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        $text = (($scpOutput | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine).Trim()
        throw "$Description failed: $text"
    }
}

function Copy-ItemsToRemote {
    param(
        [Parameter(Mandatory)]
        [System.IO.FileSystemInfo[]]$Items,

        [Parameter(Mandatory)]
        [string]$RemoteTarget,

        [Parameter(Mandatory)]
        [int]$TimeoutSeconds,

        [Parameter(Mandatory)]
        [string]$Label
    )

    if ($Items.Count -eq 0) {
        throw "No $Label items were available for staging."
    }
    foreach ($item in $Items) {
        Invoke-Scp -Description "Stage $Label item $($item.Name)" -Arguments @(
            '-q',
            '-r',
            '-o', 'BatchMode=yes',
            '-o', "ConnectTimeout=$TimeoutSeconds",
            $item.FullName,
            $RemoteTarget
        )
    }
}

function Get-AppRuntimeItems {
    param(
        [Parameter(Mandatory)]
        [string]$ExecutablePath
    )

    $appDirectory = Split-Path -Parent $ExecutablePath
    $items = New-Object System.Collections.Generic.List[System.IO.FileSystemInfo]
    foreach ($item in Get-ChildItem -LiteralPath $appDirectory -Force) {
        if ($item.PSIsContainer -and $item.Name -in @('resources', 'locales')) {
            $items.Add($item)
            continue
        }
        if (-not $item.PSIsContainer -and $item.Extension -in @('.exe', '.dll', '.json', '.dat')) {
            $items.Add($item)
        }
    }

    if (-not ($items | Where-Object { $_.FullName -eq $ExecutablePath })) {
        $items.Add((Get-Item -LiteralPath $ExecutablePath))
    }
    return @($items | Sort-Object FullName -Unique)
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory)]
        [string]$Path,

        [Parameter(Mandatory)]
        $Value
    )

    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    [System.IO.File]::WriteAllText(
        $Path,
        (($Value | ConvertTo-Json -Depth 12) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
}

function Get-RemoteJsonPayload {
    param(
        [AllowEmptyString()]
        [string]$Output,

        [Parameter(Mandatory)]
        [string]$Label
    )

    $start = $Output.IndexOf('{')
    if ($start -lt 0) {
        throw "$Label did not return a JSON payload."
    }
    try {
        return ($Output.Substring($start) | ConvertFrom-Json)
    } catch {
        throw "$Label returned invalid JSON: $($_.Exception.Message)"
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$resolvedAppPath = Resolve-RequiredPath -PathValue $AppPath -Label 'AppPath'
$resolvedModelDirectory = Resolve-RequiredPath -PathValue $ModelDirectory -Label 'ModelDirectory' -Directory
$resolvedWavPath = Resolve-RequiredPath -PathValue $WavPath -Label 'WavPath'
$resolvedModulePath = Resolve-RequiredPath -PathValue $RemoteModulePath -Label 'RemoteModulePath'
$resolvedConfigPath = Resolve-RequiredPath -PathValue $RemoteConfigPath -Label 'RemoteConfigPath'
$resolvedArtifactRoot = Resolve-OutputRoot -PathValue $ArtifactRoot

if ([System.IO.Path]::GetExtension($resolvedAppPath) -ne '.exe') {
    throw 'AppPath must be a Windows executable.'
}
if ([System.IO.Path]::GetExtension($resolvedWavPath) -ne '.wav') {
    throw 'WavPath must be a WAV fixture.'
}
if ([System.IO.Path]::GetFileName($ModelId) -ne $ModelId) {
    throw 'ModelId must be a single model directory name.'
}

$selectedModelDirectory = Resolve-RequiredPath -PathValue (Join-Path $resolvedModelDirectory $ModelId) -Label 'selected model directory' -Directory

$installedAppRoot = Join-Path $env:LOCALAPPDATA 'Verbatim'
if ($resolvedAppPath.StartsWith($installedAppRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing to stage the controller workstation installed Verbatim app as a test payload.'
}

$runnerScript = Resolve-RequiredPath -PathValue (Join-Path $PSScriptRoot 'run-windows-golden-dictation.ps1') -Label 'interactive runner'
$playbackScript = Resolve-RequiredPath -PathValue (Join-Path $PSScriptRoot 'play-wav-to-windows-device.ps1') -Label 'WASAPI playback script'
$controlledTargetScript = Resolve-RequiredPath -PathValue (Join-Path $PSScriptRoot 'controlled-winforms-target.ps1') -Label 'controlled desktop target script'
$playbackSource = Resolve-RequiredPath -PathValue (Join-Path $PSScriptRoot 'windows-wasapi-playback.cs') -Label 'WASAPI playback source'
$artifactChecker = Resolve-RequiredPath -PathValue (Join-Path $PSScriptRoot 'check-artifacts.ts') -Label 'artifact checker'

Import-Module $resolvedModulePath -Force
$remote = Get-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath
if ([string]$remote.OS -ne 'windows') {
    throw "The golden dictation lane requires a Windows physical runner, got '$($remote.OS)'."
}

$runId = ((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$localRunRoot = Join-Path $resolvedArtifactRoot $runId
$localEvidenceDirectory = Join-Path $localRunRoot 'evidence'
$localStageDirectory = Join-Path $localRunRoot 'stage'
$remoteRunRoot = "C:\AgentArtifacts\whiteknight-tasks\$runId"
$remotePayloadDirectory = Join-Path $remoteRunRoot 'payload'
$remoteAppDirectory = Join-Path $remotePayloadDirectory 'app'
$remoteModelDirectory = Join-Path $remotePayloadDirectory 'models'
$remoteScriptsDirectory = Join-Path $remotePayloadDirectory 'scripts'
$remoteEvidenceDirectory = Join-Path $remoteRunRoot 'evidence'
$remoteWorkDirectory = Join-Path $remoteRunRoot 'work'
$remoteConfigPath = Join-Path $remotePayloadDirectory 'golden-dictation.config.json'
$remoteRunnerPath = Join-Path $remoteScriptsDirectory 'run-windows-golden-dictation.ps1'
$remoteDispatcherPath = Join-Path $remoteRunRoot 'start-golden-dictation.ps1'
$remoteAppPath = Join-Path $remoteAppDirectory (Split-Path -Leaf $resolvedAppPath)
$remoteWavPath = Join-Path $remotePayloadDirectory 'fixture.wav'
$remoteScpRoot = ConvertTo-WindowsScpPath -WindowsPath $remoteRunRoot
$remoteScpPayload = ConvertTo-WindowsScpPath -WindowsPath $remotePayloadDirectory
$remoteScpApp = ConvertTo-WindowsScpPath -WindowsPath $remoteAppDirectory
$remoteScpModels = ConvertTo-WindowsScpPath -WindowsPath $remoteModelDirectory
$remoteScpScripts = ConvertTo-WindowsScpPath -WindowsPath $remoteScriptsDirectory
$remoteScpEvidence = ConvertTo-WindowsScpPath -WindowsPath $remoteEvidenceDirectory
$scpTarget = ConvertTo-ScpTarget -Remote $remote

New-Item -ItemType Directory -Force -Path $localEvidenceDirectory, $localStageDirectory | Out-Null

$controllerReport = [ordered]@{
    schema_version = 1
    runner = 'whiteknight_golden_dictation_controller'
    run_id = $runId
    remote = $WhiteKnightName
    isolated_profile = $true
    installed_app_touched = $false
    virtual_audio_defaults_changed = $false
    transcript_recorded = $false
    audio_recorded = $false
    staged_runtime_file_count = 0
    staged_model_item_count = 0
    dispatcher_started = $false
    report_received = $false
    artifact_gate_passed = $false
    remote_cleanup_attempted = $false
    remote_cleanup_succeeded = $false
    failure_class = $null
}

$remotePrepared = $false
try {
    $preflightCommand = @"
`$ErrorActionPreference = 'Stop'
`$verbatimProcesses = @(
    Get-Process -Name verbatim -ErrorAction SilentlyContinue |
        ForEach-Object { [ordered]@{ id = `$_.Id; path = [string]`$_.Path } }
)
[ordered]@{
    schema_version = 1
    verbatim_process_count = `$verbatimProcesses.Count
} | ConvertTo-Json -Compress
"@
    $preflight = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $preflightCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout 60
    if ($preflight.ExitCode -ne 0) {
        throw "WhiteKnight preflight failed."
    }
    $preflightPayload = Get-RemoteJsonPayload -Output ([string]$preflight.Output) -Label 'WhiteKnight preflight'
    if ([int]$preflightPayload.verbatim_process_count -ne 0) {
        throw 'Refusing to start a staged build while any Verbatim process is already running on WhiteKnight.'
    }

    $prepareCommand = "New-Item -ItemType Directory -Force -Path $(ConvertTo-PowerShellLiteral $remoteAppDirectory), $(ConvertTo-PowerShellLiteral $remoteModelDirectory), $(ConvertTo-PowerShellLiteral $remoteScriptsDirectory), $(ConvertTo-PowerShellLiteral $remoteEvidenceDirectory), $(ConvertTo-PowerShellLiteral $remoteWorkDirectory) | Out-Null"
    $prepare = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $prepareCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout 60
    if ($prepare.ExitCode -ne 0) {
        throw 'Failed to create the isolated WhiteKnight task directory.'
    }
    $remotePrepared = $true

    $remotePlaybackSourcePath = Join-Path $remoteScriptsDirectory 'windows-wasapi-playback.cs'
    Invoke-Scp -Description 'Stage Core Audio endpoint probe' -Arguments @('-q', '-o', 'BatchMode=yes', '-o', "ConnectTimeout=$ConnectTimeoutSeconds", $playbackSource, ("{0}:{1}" -f $scpTarget, $remoteScpScripts))
    $endpointProbeCommand = @"
`$ErrorActionPreference = 'Stop'
Add-Type -Path $(ConvertTo-PowerShellLiteral $remotePlaybackSourcePath)
`$inputName = $(ConvertTo-PowerShellLiteral $InputDeviceName)
`$outputName = $(ConvertTo-PowerShellLiteral $OutputDeviceName)
`$captureNames = @([Verbatim.NativeSmoke.WasapiWavPlayback]::ListActiveCaptureDevices())
`$renderNames = @([Verbatim.NativeSmoke.WasapiWavPlayback]::ListActiveRenderDevices())
[ordered]@{
    schema_version = 1
    input_device_present = (`$captureNames -contains `$inputName)
    output_device_present = (`$renderNames -contains `$outputName)
} | ConvertTo-Json -Compress
"@
    $endpointProbe = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $endpointProbeCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout 60
    if ($endpointProbe.ExitCode -ne 0) {
        throw 'The Core Audio endpoint preflight failed on WhiteKnight.'
    }
    $endpointProbePayload = Get-RemoteJsonPayload -Output ([string]$endpointProbe.Output) -Label 'WhiteKnight Core Audio endpoint preflight'
    if ($endpointProbePayload.input_device_present -ne $true) {
        throw 'The requested virtual microphone is not present in WhiteKnight Core Audio capture endpoints.'
    }
    if ($endpointProbePayload.output_device_present -ne $true) {
        throw 'The requested virtual render endpoint is not present in WhiteKnight Core Audio render endpoints.'
    }

    $runtimeItems = @(Get-AppRuntimeItems -ExecutablePath $resolvedAppPath)
    $controllerReport.staged_runtime_file_count = $runtimeItems.Count
    Copy-ItemsToRemote -Items $runtimeItems -RemoteTarget ("{0}:{1}" -f $scpTarget, $remoteScpApp) -TimeoutSeconds $ConnectTimeoutSeconds -Label 'staged app runtime'

    $modelItems = @((Get-Item -LiteralPath $selectedModelDirectory))
    $controllerReport.staged_model_item_count = $modelItems.Count
    Copy-ItemsToRemote -Items $modelItems -RemoteTarget ("{0}:{1}" -f $scpTarget, $remoteScpModels) -TimeoutSeconds $ConnectTimeoutSeconds -Label 'selected local model'
    Invoke-Scp -Description 'Stage deterministic WAV fixture' -Arguments @('-q', '-o', 'BatchMode=yes', '-o', "ConnectTimeout=$ConnectTimeoutSeconds", $resolvedWavPath, ("{0}:{1}" -f $scpTarget, (ConvertTo-WindowsScpPath -WindowsPath $remoteWavPath)))

    $interactiveScriptItems = @(
        (Get-Item -LiteralPath $runnerScript)
        (Get-Item -LiteralPath $playbackScript)
        (Get-Item -LiteralPath $controlledTargetScript)
    )
    Copy-ItemsToRemote -Items $interactiveScriptItems -RemoteTarget ("{0}:{1}" -f $scpTarget, $remoteScpScripts) -TimeoutSeconds $ConnectTimeoutSeconds -Label 'interactive test script'

    $remoteConfig = [ordered]@{
        app_path = $remoteAppPath
        model_dir = $remoteModelDirectory
        wav_path = $remoteWavPath
        task_root = $remoteRunRoot
        run_root = $remoteWorkDirectory
        evidence_dir = $remoteEvidenceDirectory
        input_device_name = $InputDeviceName
        output_device_name = $OutputDeviceName
        model_id = $ModelId
    }
    $localConfigPath = Join-Path $localStageDirectory 'golden-dictation.config.json'
    Write-JsonFile -Path $localConfigPath -Value $remoteConfig
    Invoke-Scp -Description 'Stage interactive test configuration' -Arguments @('-q', '-o', 'BatchMode=yes', '-o', "ConnectTimeout=$ConnectTimeoutSeconds", $localConfigPath, ("{0}:{1}" -f $scpTarget, $remoteScpPayload))

    $stagingVerificationCommand = @"
[ordered]@{
    schema_version = 1
    app_executable = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral $remoteAppPath) -PathType Leaf
    app_library = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral (Join-Path $remoteAppDirectory 'verbatim_app_lib.dll')) -PathType Leaf
    vad_resource = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral (Join-Path $remoteAppDirectory 'resources\models\silero_vad_v4.onnx')) -PathType Leaf
    local_model_root = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral (Join-Path $remoteModelDirectory $ModelId)) -PathType Container
    deterministic_wav = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral $remoteWavPath) -PathType Leaf
    controlled_target_script = Test-Path -LiteralPath $(ConvertTo-PowerShellLiteral (Join-Path $remoteScriptsDirectory 'controlled-winforms-target.ps1')) -PathType Leaf
} | ConvertTo-Json -Compress
"@
    $stagingVerification = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $stagingVerificationCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout 60
    if ($stagingVerification.ExitCode -ne 0) {
        throw 'The staged WhiteKnight payload verification failed.'
    }
    $stagingPayload = Get-RemoteJsonPayload -Output ([string]$stagingVerification.Output) -Label 'WhiteKnight staged payload verification'
    foreach ($requiredPayloadField in @('app_executable', 'app_library', 'vad_resource', 'local_model_root', 'deterministic_wav', 'controlled_target_script')) {
        if ($stagingPayload.$requiredPayloadField -ne $true) {
            throw "The staged WhiteKnight payload is missing required $requiredPayloadField evidence."
        }
    }

    $dispatcher = @"
param(
    [Parameter(Mandatory=`$true)][string]`$RunRoot,
    [Parameter(Mandatory=`$true)][string]`$RunId,
    [Parameter(Mandatory=`$true)][int]`$TimeoutSeconds
)

`$ErrorActionPreference = 'Stop'
`$ProgressPreference = 'SilentlyContinue'
`$runnerPath = Join-Path `$RunRoot 'payload\scripts\run-windows-golden-dictation.ps1'
`$configPath = Join-Path `$RunRoot 'payload\golden-dictation.config.json'
`$evidencePath = Join-Path `$RunRoot 'evidence'
`$reportPath = Join-Path `$evidencePath 'golden-dictation.json'
`$consolePath = Join-Path `$RunRoot 'golden-dictation.console.log'
`$wrapperPath = Join-Path `$RunRoot 'run-golden-dictation.ps1'
`$taskName = "CodexWhiteKnightGoldenDictation-`$RunId"
`$powerShellPath = Join-Path `$env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
`$runAsUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name
`$taskCreated = `$false
`$taskStarted = `$false
try {
    `$escapedRunnerPath = `$runnerPath.Replace("'", "''")
    `$escapedConfigPath = `$configPath.Replace("'", "''")
    `$escapedConsolePath = `$consolePath.Replace("'", "''")
    `$wrapperLines = @(
        '`$ErrorActionPreference = ''Stop''',
        ("& '{0}' -ConfigPath '{1}' *> '{2}'" -f `$escapedRunnerPath, `$escapedConfigPath, `$escapedConsolePath)
    )
    Set-Content -LiteralPath `$wrapperPath -Value `$wrapperLines -Encoding utf8
    `$taskCommand = ('"{0}" -NoLogo -NoProfile -STA -ExecutionPolicy Bypass -File "{1}"' -f `$powerShellPath, `$wrapperPath)
    `$createOutput = & `"`$env:WINDIR\System32\schtasks.exe`" /Create /TN `$taskName /SC ONCE /ST 23:59 /TR `$taskCommand /RU `$runAsUser /IT /F 2>&1
    if (`$LASTEXITCODE -ne 0) { throw 'Failed to create the interactive golden-dictation task.' }
    `$taskCreated = `$true
    `$runOutput = & `"`$env:WINDIR\System32\schtasks.exe`" /Run /TN `$taskName 2>&1
    if (`$LASTEXITCODE -ne 0) { throw 'Failed to start the interactive golden-dictation task.' }
    `$taskStarted = `$true

    `$deadline = [DateTime]::UtcNow.AddSeconds(`$TimeoutSeconds)
    while ([DateTime]::UtcNow -lt `$deadline -and -not (Test-Path -LiteralPath `$reportPath -PathType Leaf)) {
        Start-Sleep -Milliseconds 500
    }
    `$report = `$null
    if (Test-Path -LiteralPath `$reportPath -PathType Leaf) {
        try { `$report = Get-Content -Raw -LiteralPath `$reportPath | ConvertFrom-Json } catch { `$report = `$null }
    }
    `$evidenceFiles = @()
    if (Test-Path -LiteralPath `$evidencePath -PathType Container) {
        `$evidenceFiles = @(Get-ChildItem -LiteralPath `$evidencePath -File -ErrorAction SilentlyContinue | ForEach-Object { `$_.Name })
    }
    [ordered]@{
        schema_version = 1
        task_created = `$taskCreated
        task_started = `$taskStarted
        report_present = (`$null -ne `$report)
        report_failure_class = if (`$report) { `$report.failure_class } else { 'report_missing_or_invalid' }
        report_case_count = if (`$report -and `$report.cases) { @(`$report.cases).Count } else { 0 }
        evidence_files = @(`$evidenceFiles)
    } | ConvertTo-Json -Compress
} finally {
    if (`$taskCreated) {
        & `"`$env:WINDIR\System32\schtasks.exe`" /Delete /TN `$taskName /F 2>`$null | Out-Null
    }
}
"@
    $localDispatcherPath = Join-Path $localStageDirectory 'start-golden-dictation.ps1'
    [System.IO.File]::WriteAllText($localDispatcherPath, $dispatcher, [System.Text.UTF8Encoding]::new($false))
    Invoke-Scp -Description 'Stage WhiteKnight interactive dispatcher' -Arguments @('-q', '-o', 'BatchMode=yes', '-o', "ConnectTimeout=$ConnectTimeoutSeconds", $localDispatcherPath, ("{0}:{1}" -f $scpTarget, $remoteScpRoot))

    $dispatchCommand = "& $(ConvertTo-PowerShellLiteral $remoteDispatcherPath) -RunRoot $(ConvertTo-PowerShellLiteral $remoteRunRoot) -RunId $(ConvertTo-PowerShellLiteral $runId) -TimeoutSeconds $TaskTimeoutSeconds"
    $dispatch = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $dispatchCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout ($TaskTimeoutSeconds + 90)
    if ($dispatch.ExitCode -ne 0) {
        throw 'WhiteKnight interactive dispatcher failed before it could produce a safe receipt.'
    }
    $dispatchPayload = Get-RemoteJsonPayload -Output ([string]$dispatch.Output) -Label 'WhiteKnight interactive dispatcher'
    $controllerReport.dispatcher_started = ($dispatchPayload.task_created -eq $true -and $dispatchPayload.task_started -eq $true)

    $allowedEvidenceNames = @(
        'golden-dictation.json',
        'stable-focus.insertion.jsonl',
        'stable-focus.playback.json',
        'focus-switch.insertion.jsonl',
        'focus-switch.playback.json',
        'clipboard-mutation.insertion.jsonl',
        'clipboard-mutation.playback.json'
    )
    $remoteEvidenceNames = @($dispatchPayload.evidence_files | ForEach-Object { [string]$_ })
    foreach ($evidenceName in $allowedEvidenceNames) {
        if ($remoteEvidenceNames -contains $evidenceName) {
            $remoteEvidencePath = Join-Path $remoteEvidenceDirectory $evidenceName
            Invoke-Scp -Description "Copy safe evidence $evidenceName" -Arguments @('-q', '-o', 'BatchMode=yes', '-o', "ConnectTimeout=$ConnectTimeoutSeconds", ("{0}:{1}" -f $scpTarget, (ConvertTo-WindowsScpPath -WindowsPath $remoteEvidencePath)), $localEvidenceDirectory)
        }
    }

    $controllerReport.report_received = Test-Path -LiteralPath (Join-Path $localEvidenceDirectory 'golden-dictation.json') -PathType Leaf
    if (-not $controllerReport.report_received) {
        throw 'The interactive runner did not produce golden-dictation.json.'
    }

    & bun $artifactChecker --dir $localEvidenceDirectory --require-golden-dictation --golden-dictation-only
    if ($LASTEXITCODE -ne 0) {
        throw 'The golden-dictation evidence gate failed.'
    }
    $controllerReport.artifact_gate_passed = $true
}
catch {
    $controllerReport.failure_class = 'golden_dictation_controller_failed'
    $controllerReport.failure_message = $_.Exception.Message
    throw
}
finally {
    if ($remotePrepared) {
        $controllerReport.remote_cleanup_attempted = $true
        $cleanupCommand = @"
`$target = $(ConvertTo-PowerShellLiteral $remoteRunRoot)
`$approvedRoot = 'C:\AgentArtifacts\whiteknight-tasks'
if (-not `$target.StartsWith((`$approvedRoot + '\'), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing to remove a WhiteKnight path outside the approved task root.'
}
if (Test-Path -LiteralPath `$target -PathType Container) {
    Remove-Item -LiteralPath `$target -Recurse -Force
}
"@
        $cleanup = Invoke-CodexRemote -Name $WhiteKnightName -ConfigPath $resolvedConfigPath -Command $cleanupCommand -AllowFailure -ConnectTimeout $ConnectTimeoutSeconds -CommandTimeout 90
        $controllerReport.remote_cleanup_succeeded = ($cleanup.ExitCode -eq 0)
        if ($cleanup.ExitCode -ne 0 -and $null -eq $controllerReport.failure_class) {
            $controllerReport.failure_class = 'remote_cleanup_failed'
        }
    }
    Write-JsonFile -Path (Join-Path $localRunRoot 'controller-report.json') -Value $controllerReport
}

if (-not $controllerReport.artifact_gate_passed) {
    exit 1
}

Write-Output "WhiteKnight golden dictation passed. Safe evidence: $localEvidenceDirectory"
