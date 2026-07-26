[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$ConfigPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

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

function Get-AbsoluteOutputPath {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue,

        [Parameter(Mandatory)]
        [string]$Label
    )

    if (-not [System.IO.Path]::IsPathRooted($PathValue)) {
        throw "$Label must be an absolute path."
    }
    return [System.IO.Path]::GetFullPath($PathValue)
}

function Get-ApprovedTaskRoot {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue
    )

    $taskRoot = Resolve-RequiredPath -PathValue $PathValue -Label 'task_root' -Directory
    $approvedParent = [System.IO.Path]::GetFullPath('C:\AgentArtifacts\whiteknight-tasks').TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    $candidateParent = (Split-Path -Parent $taskRoot).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    if (-not [string]::Equals($candidateParent, $approvedParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'task_root must be a direct child of the approved WhiteKnight task parent.'
    }

    return $taskRoot
}

function Get-TaskChildOutputPath {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue,

        [Parameter(Mandatory)]
        [string]$TaskRoot,

        [Parameter(Mandatory)]
        [string]$ChildName,

        [Parameter(Mandatory)]
        [string]$Label
    )

    $candidate = Get-AbsoluteOutputPath -PathValue $PathValue -Label $Label
    $expected = [System.IO.Path]::GetFullPath((Join-Path $TaskRoot $ChildName))
    if (-not [string]::Equals($candidate, $expected, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Label must be the '$ChildName' directory inside task_root."
    }

    return $candidate
}

function Assert-NotReparsePoint {
    param(
        [Parameter(Mandatory)]
        [string]$PathValue,

        [Parameter(Mandatory)]
        [string]$Label
    )

    if (-not (Test-Path -LiteralPath $PathValue)) {
        return
    }

    $item = Get-Item -LiteralPath $PathValue -Force
    if (([System.IO.FileAttributes]$item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must not be a reparse point."
    }
}

function Wait-Until {
    param(
        [Parameter(Mandatory)]
        [scriptblock]$Condition,

        [Parameter(Mandatory)]
        [int]$TimeoutMilliseconds,

        [Parameter(Mandatory)]
        [string]$Description
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for $Description after ${TimeoutMilliseconds}ms."
}

function Add-NativeSmokeWindowInterop {
    if ('Verbatim.NativeSmoke.WindowInterop' -as [type]) {
        return
    }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace Verbatim.NativeSmoke
{
    public static class WindowInterop
    {
        [DllImport("user32.dll")]
        private static extern bool SetForegroundWindow(IntPtr handle);

        [DllImport("user32.dll")]
        private static extern bool BringWindowToTop(IntPtr handle);

        [DllImport("user32.dll")]
        private static extern IntPtr SetFocus(IntPtr handle);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);

        [DllImport("user32.dll")]
        private static extern bool AttachThreadInput(uint attachThreadId, uint attachToThreadId, bool attach);

        [DllImport("kernel32.dll")]
        private static extern uint GetCurrentThreadId();

        [DllImport("user32.dll")]
        private static extern bool ShowWindow(IntPtr handle, int command);

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool OpenClipboard(IntPtr owner);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool CloseClipboard();

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool EmptyClipboard();

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint CountClipboardFormats();

        [DllImport("user32.dll")]
        private static extern bool IsClipboardFormatAvailable(uint format);

        [DllImport("user32.dll")]
        private static extern IntPtr GetClipboardData(uint format);

        [DllImport("kernel32.dll")]
        private static extern IntPtr GlobalLock(IntPtr memory);

        [DllImport("kernel32.dll")]
        private static extern bool GlobalUnlock(IntPtr memory);

        public static bool Activate(IntPtr handle)
        {
            uint ignoredProcessId;
            var foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out ignoredProcessId);
            var currentThread = GetCurrentThreadId();
            var attached = foregroundThread != 0 && foregroundThread != currentThread &&
                AttachThreadInput(currentThread, foregroundThread, true);
            try
            {
                ShowWindow(handle, 9);
                BringWindowToTop(handle);
                SetForegroundWindow(handle);
                SetFocus(handle);
                return GetForegroundWindow() == handle;
            }
            finally
            {
                if (attached)
                {
                    AttachThreadInput(currentThread, foregroundThread, false);
                }
            }
        }

        public static IntPtr Foreground()
        {
            return GetForegroundWindow();
        }

        public static bool IsClipboardEmpty()
        {
            for (var attempt = 0; attempt < 20; attempt++)
            {
                if (!OpenClipboard(IntPtr.Zero))
                {
                    System.Threading.Thread.Sleep(50);
                    continue;
                }

                try
                {
                    return CountClipboardFormats() == 0;
                }
                finally
                {
                    CloseClipboard();
                }
            }

            return false;
        }

        public static bool ClearClipboardIfMatches(string expected)
        {
            const uint CfUnicodeText = 13;
            for (var attempt = 0; attempt < 20; attempt++)
            {
                if (!OpenClipboard(IntPtr.Zero))
                {
                    System.Threading.Thread.Sleep(50);
                    continue;
                }

                try
                {
                    if (!IsClipboardFormatAvailable(CfUnicodeText))
                    {
                        return false;
                    }
                    var clipboardData = GetClipboardData(CfUnicodeText);
                    if (clipboardData == IntPtr.Zero)
                    {
                        return false;
                    }
                    var textPointer = GlobalLock(clipboardData);
                    if (textPointer == IntPtr.Zero)
                    {
                        return false;
                    }
                    try
                    {
                        var observed = Marshal.PtrToStringUni(textPointer);
                        if (!String.Equals(observed, expected, StringComparison.Ordinal))
                        {
                            return false;
                        }
                        return EmptyClipboard();
                    }
                    finally
                    {
                        GlobalUnlock(clipboardData);
                    }
                }
                finally
                {
                    CloseClipboard();
                }
            }

            return false;
        }
    }
}
'@ -ErrorAction Stop
}

function Get-ControlledTargetSnapshot {
    param(
        [Parameter(Mandatory)]
        $Target
    )

    try {
        if (-not (Test-Path -LiteralPath $Target.StatePath -PathType Leaf)) {
            return $null
        }
        $snapshot = Get-Content -Raw -LiteralPath $Target.StatePath | ConvertFrom-Json
        if (
            [int]$snapshot.schema_version -ne 1 -or
            [int]$snapshot.process_id -ne $Target.Process.Id -or
            [int]$snapshot.sequence -lt 1 -or
            [int64]$snapshot.window_handle -le 0 -or
            [int]$snapshot.initial_length -lt 0 -or
            [int]$snapshot.current_length -lt 0 -or
            $null -eq $snapshot.form_contains_focus -or
            $null -eq $snapshot.text_box_focused
        ) {
            return $null
        }
        return $snapshot
    } catch {
        # The target atomically refreshes a tiny metadata file. A retry is safer
        # than interpreting a concurrent write as a target failure.
        return $null
    }
}

function Set-ControlledTargetFocus {
    param(
        [Parameter(Mandatory)]
        $Target,

        [Parameter(Mandatory)]
        [string]$Label,

        [Parameter(Mandatory)]
        [System.Collections.IDictionary]$TargetEvidence
    )

    Wait-Until -TimeoutMilliseconds 10000 -Description "$Label window" -Condition {
        if ($Target.Process.HasExited) {
            $TargetEvidence.process_exited_before_window = $true
            throw "$Label target process exited before its window was available."
        }
        $snapshot = Get-ControlledTargetSnapshot -Target $Target
        $windowObserved = $null -ne $snapshot
        if ($windowObserved) {
            $Target.WindowHandle = [System.IntPtr]::new([int64]$snapshot.window_handle)
            $Target.InitialLength = [int]$snapshot.initial_length
            $Target.LastSequence = [int]$snapshot.sequence
            $TargetEvidence.main_window_observed = $true
        }
        return $windowObserved
    }

    $TargetEvidence.focus_activation_requested = $true
    $TargetEvidence.focus_confirmed = $false
    Wait-Until -TimeoutMilliseconds 5000 -Description "$Label foreground activation" -Condition {
        if ($Target.Process.HasExited) {
            $TargetEvidence.process_exited_before_window = $true
            throw "$Label target process exited during foreground activation."
        }
        [System.IO.File]::WriteAllText($Target.FocusPath, '', [System.Text.UTF8Encoding]::new($false))
        [void][Verbatim.NativeSmoke.WindowInterop]::Activate($Target.WindowHandle)
        $snapshot = Get-ControlledTargetSnapshot -Target $Target
        $TargetEvidence.focus_confirmed = (
            $null -ne $snapshot -and
            [bool]$snapshot.form_contains_focus -and
            [bool]$snapshot.text_box_focused -and
            [Verbatim.NativeSmoke.WindowInterop]::Foreground().ToInt64() -eq $Target.WindowHandle.ToInt64()
        )
        return $TargetEvidence.focus_confirmed
    }
}

function Start-ControlledTarget {
    param(
        [Parameter(Mandatory)]
        [string]$CaseDirectory,

        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [string]$TargetScriptPath,

        [Parameter(Mandatory)]
        [System.Collections.IDictionary]$TargetEvidence
    )

    if ([Threading.Thread]::CurrentThread.ApartmentState -ne [Threading.ApartmentState]::STA) {
        throw 'The controlled WinForms target requires an STA desktop thread.'
    }

    $TargetEvidence.process_started = $false
    $TargetEvidence.main_window_observed = $false
    $TargetEvidence.process_exited_before_window = $false
    $TargetEvidence.focus_activation_requested = $false
    $TargetEvidence.focus_confirmed = $false
    $statePath = Join-Path $CaseDirectory "$Name.target-state.json"
    $stopPath = Join-Path $CaseDirectory "$Name.target-stop"
    $focusPath = Join-Path $CaseDirectory "$Name.target-focus"
    $targetProcess = $null
    try {
        $targetProcess = Start-Process -FilePath (Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe') -ArgumentList @('-NoLogo', '-NoProfile', '-STA', '-ExecutionPolicy', 'Bypass', '-File', $TargetScriptPath, '-Name', $Name, '-StatePath', $statePath, '-StopPath', $stopPath, '-FocusPath', $focusPath) -PassThru
        $TargetEvidence.process_started = $true
        $target = [pscustomobject]@{
            Process = $targetProcess
            StatePath = $statePath
            StopPath = $stopPath
            FocusPath = $focusPath
            WindowHandle = [System.IntPtr]::Zero
            InitialLength = 0
            LastSequence = 0
            LastSnapshot = $null
            Evidence = $TargetEvidence
        }
        Set-ControlledTargetFocus -Target $target -Label $Name -TargetEvidence $TargetEvidence
        return $target
    } catch {
        if ($null -ne $targetProcess) {
            try {
                Stop-Process -Id $targetProcess.Id -Force -ErrorAction SilentlyContinue
            } catch { }
        }
        throw
    }
}

function Get-ControlledTargetState {
    param(
        [Parameter(Mandatory)]
        $Target
    )

    $latestBeforeRefresh = Get-ControlledTargetSnapshot -Target $Target
    $previousSequence = if ($null -eq $latestBeforeRefresh) {
        [int]$Target.LastSequence
    } else {
        [int]$latestBeforeRefresh.sequence
    }
    Wait-Until -TimeoutMilliseconds 5000 -Description 'controlled target metadata refresh' -Condition {
        $candidate = Get-ControlledTargetSnapshot -Target $Target
        if ($null -eq $candidate) {
            return $false
        }
        if ([int]$candidate.sequence -le $previousSequence) {
            return $false
        }
        $Target.LastSnapshot = $candidate
        return $true
    }
    $Target.LastSequence = [int]$Target.LastSnapshot.sequence
    $currentLength = [int]$Target.LastSnapshot.current_length
    return [pscustomobject]@{
        Mutated = $currentLength -ne $Target.InitialLength
        HasNonemptyInsertion = $currentLength -gt $Target.InitialLength
    }
}

function Close-ControlledTarget {
    param(
        [Parameter(Mandatory)]
        $Target
    )

    if ($Target.Process.HasExited) {
        return
    }
    [System.IO.File]::WriteAllText($Target.StopPath, '', [System.Text.UTF8Encoding]::new($false))
    if (-not $Target.Process.WaitForExit(10000)) {
        Stop-Process -Id $Target.Process.Id -Force -ErrorAction SilentlyContinue
        if (-not $Target.Process.WaitForExit(5000)) {
            throw 'Failed to close the controlled target process.'
        }
    }
}

function Invoke-AppToggle {
    param(
        [Parameter(Mandatory)]
        [string]$AppPath
    )

    $toggleProcess = Start-Process -FilePath $AppPath -ArgumentList '--toggle-transcription' -PassThru
    if (-not $toggleProcess.WaitForExit(15000)) {
        Stop-Process -Id $toggleProcess.Id -Force -ErrorAction SilentlyContinue
        throw 'The Verbatim toggle command did not exit.'
    }
    if ($toggleProcess.ExitCode -ne 0) {
        throw "The Verbatim toggle command failed with exit code $($toggleProcess.ExitCode)."
    }
}

function Start-IsolatedApp {
    param(
        [Parameter(Mandatory)]
        [string]$AppPath,

        [Parameter(Mandatory)]
        [string]$CaseDirectory,

        [Parameter(Mandatory)]
        [string]$ModelDirectory,

        [Parameter(Mandatory)]
        [string]$ModelId,

        [Parameter(Mandatory)]
        [string]$InputDeviceName,

        [Parameter(Mandatory)]
        [string]$CaseName,

        [Parameter(Mandatory)]
        [string]$ReceiptPath,

        [Parameter(Mandatory)]
        [System.Collections.IDictionary]$StartupEvidence,

        [string]$BarrierDirectory,

        [string]$BarrierStages
    )

    $dataDirectory = Join-Path $CaseDirectory 'isolated-data'
    $statusPath = Join-Path $CaseDirectory 'native-smoke-status.json'
    New-Item -ItemType Directory -Force -Path $dataDirectory | Out-Null

    $StartupEvidence.process_started = $false
    $StartupEvidence.settings_file_observed = $false
    $StartupEvidence.app_exited_before_settings = $false
    $StartupEvidence.app_exit_code = $null

    $env:VERBATIM_SMOKE_STATUS_PATH = $statusPath
    $env:VERBATIM_SMOKE_DATA_DIR = $dataDirectory
    $env:VERBATIM_SMOKE_SELECTED_MICROPHONE = $InputDeviceName
    $env:VERBATIM_SMOKE_SELECTED_MODEL = $ModelId
    $env:VERBATIM_SMOKE_MODEL_DIR = $ModelDirectory
    $env:VERBATIM_SMOKE_INSERTION_CASE = $CaseName
    $env:VERBATIM_SMOKE_INSERTION_RECEIPT_PATH = $ReceiptPath
    $env:VERBATIM_SMOKE_BARRIER_TIMEOUT_MS = '15000'
    if ([string]::IsNullOrWhiteSpace($BarrierDirectory)) {
        Remove-Item -LiteralPath 'Env:VERBATIM_SMOKE_BARRIER_DIR' -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath 'Env:VERBATIM_SMOKE_BARRIER_STAGES' -ErrorAction SilentlyContinue
    } else {
        $env:VERBATIM_SMOKE_BARRIER_DIR = $BarrierDirectory
        $env:VERBATIM_SMOKE_BARRIER_STAGES = $BarrierStages
    }
    Remove-Item -LiteralPath 'Env:VERBATIM_SMOKE_REAL_INFERENCE_WAV' -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath 'Env:VERBATIM_SMOKE_EXIT_AFTER_MS' -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath 'Env:VERBATIM_SMOKE_MODEL_FIXTURE' -ErrorAction SilentlyContinue

    $appProcess = $null
    try {
        $appProcess = Start-Process -FilePath $AppPath -ArgumentList '--start-hidden' -PassThru
        $StartupEvidence.process_started = $true
        $settingsPath = Join-Path $dataDirectory 'settings_store.json'
        try {
            Wait-Until -TimeoutMilliseconds 30000 -Description "$CaseName isolated settings" -Condition {
                if ($appProcess.HasExited) {
                    $StartupEvidence.app_exited_before_settings = $true
                    $StartupEvidence.app_exit_code = [int]$appProcess.ExitCode
                    throw "The isolated Verbatim process exited before isolated settings were created."
                }
                $settingsObserved = Test-Path -LiteralPath $settingsPath -PathType Leaf
                if ($settingsObserved) {
                    $StartupEvidence.settings_file_observed = $true
                }
                return $settingsObserved
            }
        } catch {
            if ($appProcess.HasExited) {
                $StartupEvidence.app_exited_before_settings = $true
                $StartupEvidence.app_exit_code = [int]$appProcess.ExitCode
            }
            throw
        }

        $settingsStore = Get-Content -Raw -LiteralPath $settingsPath | ConvertFrom-Json
        $selectedMicrophone = [string]$settingsStore.settings.selected_microphone
        $selectedModel = [string]$settingsStore.settings.selected_model
        if ($selectedMicrophone -ne $InputDeviceName) {
            throw "The isolated app did not select the requested virtual microphone."
        }
        if ($selectedModel -ne $ModelId) {
            throw "The isolated app did not select the requested local model."
        }

        return [pscustomobject]@{
            Process = $appProcess
            DataDirectory = $dataDirectory
        }
    } catch {
        if ($null -ne $appProcess) {
            try {
                Stop-IsolatedApp -AppState ([pscustomobject]@{
                        Process = $appProcess
                        DataDirectory = $dataDirectory
                    }) -ExpectedAppPath $AppPath
            } catch { }
        }
        throw
    }
}

function Stop-IsolatedApp {
    param(
        [Parameter(Mandatory)]
        $AppState,

        [Parameter(Mandatory)]
        [string]$ExpectedAppPath
    )

    try {
        Stop-StagedAppProcess -Process $AppState.Process -ExpectedAppPath $ExpectedAppPath
    } finally {
        try {
            Stop-ProcessesUsingIsolatedDataDirectory -DataDirectory $AppState.DataDirectory
        } finally {
            Remove-Item -LiteralPath $AppState.DataDirectory -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Stop-StagedAppProcess {
    param(
        [Parameter(Mandatory)]
        [System.Diagnostics.Process]$Process,

        [Parameter(Mandatory)]
        [string]$ExpectedAppPath
    )

    $ownedProcess = Get-Process -Id $Process.Id -ErrorAction SilentlyContinue
    if ($null -eq $ownedProcess) {
        return
    }

    $observedPath = [string]$ownedProcess.Path
    if (-not [string]::Equals($observedPath, $ExpectedAppPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to stop a process outside the staged app path.'
    }

    $taskKillPath = Join-Path $env:WINDIR 'System32\taskkill.exe'
    & $taskKillPath /PID $Process.Id /T /F 2>$null | Out-Null
    Start-Sleep -Milliseconds 250
    if (Get-Process -Id $Process.Id -ErrorAction SilentlyContinue) {
        throw 'Failed to terminate the staged app process tree.'
    }
}

function Stop-ProcessesUsingIsolatedDataDirectory {
    param(
        [Parameter(Mandatory)]
        [string]$DataDirectory
    )

    # A Tauri WebView child can outlive its executable parent after a failed
    # startup. Its command line retains this unique, test-only data directory,
    # so this is a narrow last-resort cleanup without touching unrelated apps.
    $ownedProcesses = @(
        Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                -not [string]::IsNullOrWhiteSpace([string]$_.CommandLine) -and
                ([string]$_.CommandLine).IndexOf($DataDirectory, [StringComparison]::OrdinalIgnoreCase) -ge 0
            }
    )
    $taskKillPath = Join-Path $env:WINDIR 'System32\taskkill.exe'
    foreach ($ownedProcess in $ownedProcesses) {
        $processId = [int]$ownedProcess.ProcessId
        $confirmedProcess = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $processId" -ErrorAction SilentlyContinue
        if (
            $null -ne $confirmedProcess -and
            -not [string]::IsNullOrWhiteSpace([string]$confirmedProcess.CommandLine) -and
            ([string]$confirmedProcess.CommandLine).IndexOf($DataDirectory, [StringComparison]::OrdinalIgnoreCase) -ge 0
        ) {
            & $taskKillPath /PID $processId /T /F 2>$null | Out-Null
        }
    }
}

function Invoke-AudioCapture {
    param(
        [Parameter(Mandatory)]
        [string]$AppPath,

        [Parameter(Mandatory)]
        $Target,

        [Parameter(Mandatory)]
        [string]$WavPath,

        [Parameter(Mandatory)]
        [string]$OutputDeviceName,

        [Parameter(Mandatory)]
        [string]$PlaybackScriptPath,

        [Parameter(Mandatory)]
        [string]$PlaybackReceiptPath
        ,
        [Parameter(Mandatory)]
        [System.Collections.IDictionary]$CaptureEvidence
    )

    $CaptureEvidence.target_focused_before_start = $false
    $CaptureEvidence.recording_start_requested = $false
    $CaptureEvidence.target_focused_before_playback = $false
    $CaptureEvidence.playback_invoked = $false
    $CaptureEvidence.playback_completed = $false
    $CaptureEvidence.recording_stop_requested = $false
    $focusEvidence = [ordered]@{}
    Set-ControlledTargetFocus -Target $Target -Label 'dictation target' -TargetEvidence $focusEvidence
    $CaptureEvidence.target_focused_before_start = $true
    Invoke-AppToggle -AppPath $AppPath
    $CaptureEvidence.recording_start_requested = $true
    Start-Sleep -Milliseconds 750
    Set-ControlledTargetFocus -Target $Target -Label 'dictation target after start' -TargetEvidence $focusEvidence
    $CaptureEvidence.target_focused_before_playback = $true
    $CaptureEvidence.playback_invoked = $true
    & $PlaybackScriptPath -WavPath $WavPath -DeviceName $OutputDeviceName -ReceiptPath $PlaybackReceiptPath | Out-Null
    $playback = Get-Content -Raw -LiteralPath $PlaybackReceiptPath | ConvertFrom-Json
    if (-not $playback.success -or $playback.submitted_frames -ne $playback.source_frames) {
        throw 'The deterministic fixture was not fully rendered into the virtual audio output.'
    }
    $CaptureEvidence.playback_completed = $true
    Start-Sleep -Milliseconds 150
    Invoke-AppToggle -AppPath $AppPath
    $CaptureEvidence.recording_stop_requested = $true
    return $playback
}

function Wait-InsertionReceipt {
    param(
        [Parameter(Mandatory)]
        [string]$ReceiptPath,

        [Parameter(Mandatory)]
        [string]$CaseName
    )

    Wait-Until -TimeoutMilliseconds 90000 -Description "$CaseName insertion receipt" -Condition {
        return (Test-Path -LiteralPath $ReceiptPath -PathType Leaf) -and (Get-Item -LiteralPath $ReceiptPath).Length -gt 0
    }
    $lines = @(Get-Content -LiteralPath $ReceiptPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -ne 1) {
        throw "$CaseName expected exactly one insertion receipt."
    }
    return ($lines[0] | ConvertFrom-Json)
}

function Assert-InsertionReceipt {
    param(
        [Parameter(Mandatory)]
        $Receipt,

        [Parameter(Mandatory)]
        [bool]$Attempted,

        [Parameter(Mandatory)]
        [bool]$Succeeded,

        [Parameter(Mandatory)]
        [bool]$TargetVerified,

        [AllowNull()]
        $ErrorText
    )

    if ([bool]$Receipt.attempted -ne $Attempted -or [bool]$Receipt.succeeded -ne $Succeeded -or [bool]$Receipt.target_verified -ne $TargetVerified) {
        throw 'Insertion receipt did not match the expected state transition.'
    }
    $observedError = if ($null -eq $Receipt.error) { $null } else { [string]$Receipt.error }
    if ($observedError -ne $ErrorText) {
        throw 'Insertion receipt did not contain the expected reason code.'
    }
}

function Remove-CaseWork {
    param(
        [Parameter(Mandatory)]
        [string]$CaseDirectory
    )

    if (-not (Test-Path -LiteralPath $CaseDirectory -PathType Container)) {
        return
    }
    try {
        $targetFiles = @(Get-ChildItem -LiteralPath $CaseDirectory -Filter '*.txt' -File -ErrorAction Stop)
    } catch {
        return
    }
    foreach ($targetFile in $targetFiles) {
        try {
            Remove-Item -LiteralPath $targetFile.FullName -Force -ErrorAction Stop
        } catch {
            # The final run-root teardown remains the retention boundary. These
            # legacy per-case files are only best-effort interim cleanup.
        }
    }
}

function Get-SafeFailureDetail {
    param(
        [Parameter(Mandatory)]
        [System.Management.Automation.ErrorRecord]$ErrorRecord
    )

    $message = [string]$ErrorRecord.Exception.Message
    if ($message -like 'Timed out waiting for controlled target metadata refresh*') {
        return 'controlled_target_metadata_refresh_timed_out'
    }
    if ($message -eq 'Stable focus did not produce a nonempty app-driven insertion.') {
        return 'stable_target_unchanged_after_success_receipt'
    }
    if ($message -eq 'Clipboard mutation smoke requires an empty clipboard to preserve existing physical-session content.') {
        return 'clipboard_precondition_not_empty'
    }
    if ($message -like 'Failed to focus *') {
        return 'controlled_target_focus_failed'
    }
    if ($message -like 'The Verbatim toggle command*') {
        return 'toggle_command_failed'
    }
    if ($message -like 'The deterministic fixture was not fully rendered*') {
        return 'virtual_audio_playback_failed'
    }
    return 'unclassified_runner_failure'
}

$resolvedConfigPath = Resolve-RequiredPath -PathValue $ConfigPath -Label 'ConfigPath'
$config = Get-Content -Raw -LiteralPath $resolvedConfigPath | ConvertFrom-Json
$appPath = Resolve-RequiredPath -PathValue ([string]$config.app_path) -Label 'app_path'
$modelDirectory = Resolve-RequiredPath -PathValue ([string]$config.model_dir) -Label 'model_dir' -Directory
$wavPath = Resolve-RequiredPath -PathValue ([string]$config.wav_path) -Label 'wav_path'
$taskRootProperty = $config.PSObject.Properties['task_root']
if ($null -eq $taskRootProperty -or [string]::IsNullOrWhiteSpace([string]$taskRootProperty.Value)) {
    throw 'task_root is required.'
}
$taskRoot = Get-ApprovedTaskRoot -PathValue ([string]$taskRootProperty.Value)
$runRoot = Get-TaskChildOutputPath -PathValue ([string]$config.run_root) -TaskRoot $taskRoot -ChildName 'work' -Label 'run_root'
$evidenceDirectory = Get-TaskChildOutputPath -PathValue ([string]$config.evidence_dir) -TaskRoot $taskRoot -ChildName 'evidence' -Label 'evidence_dir'
$inputDeviceName = [string]$config.input_device_name
$outputDeviceName = [string]$config.output_device_name
$modelId = [string]$config.model_id
if ([string]::IsNullOrWhiteSpace($inputDeviceName) -or [string]::IsNullOrWhiteSpace($outputDeviceName) -or [string]::IsNullOrWhiteSpace($modelId)) {
    throw 'input_device_name, output_device_name, and model_id are required.'
}

$scriptDirectory = Split-Path -Parent $PSCommandPath
$playbackScriptPath = Resolve-RequiredPath -PathValue (Join-Path $scriptDirectory 'play-wav-to-windows-device.ps1') -Label 'WASAPI playback script'
$controlledTargetScriptPath = Resolve-RequiredPath -PathValue (Join-Path $scriptDirectory 'controlled-winforms-target.ps1') -Label 'controlled desktop target script'
Add-NativeSmokeWindowInterop
Assert-NotReparsePoint -PathValue $taskRoot -Label 'task_root'
Assert-NotReparsePoint -PathValue $runRoot -Label 'run_root'
Assert-NotReparsePoint -PathValue $evidenceDirectory -Label 'evidence_dir'
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
New-Item -ItemType Directory -Force -Path $evidenceDirectory | Out-Null
Assert-NotReparsePoint -PathValue $runRoot -Label 'run_root'
Assert-NotReparsePoint -PathValue $evidenceDirectory -Label 'evidence_dir'

$caseResults = @()
$stableStartupEvidence = [ordered]@{}
$focusStartupEvidence = [ordered]@{}
$clipboardStartupEvidence = [ordered]@{}
$stableTargetEvidence = [ordered]@{}
$focusOriginTargetEvidence = [ordered]@{}
$focusReplacementTargetEvidence = [ordered]@{}
$clipboardTargetEvidence = [ordered]@{}
$stableCaptureEvidence = [ordered]@{}
$focusCaptureEvidence = [ordered]@{}
$clipboardCaptureEvidence = [ordered]@{}
$report = [ordered]@{
    schema_version = 1
    runner = 'windows_interactive_golden_dictation'
    isolated_profile = $true
    transcript_recorded = $false
    audio_recorded = $false
    input_device_name = $inputDeviceName
    output_device_name = $outputDeviceName
    model_id = $modelId
    startup = [ordered]@{
        stable_focus = $stableStartupEvidence
        focus_switch = $focusStartupEvidence
        clipboard_mutation = $clipboardStartupEvidence
    }
    targets = [ordered]@{
        stable_focus = $stableTargetEvidence
        focus_origin = $focusOriginTargetEvidence
        focus_replacement = $focusReplacementTargetEvidence
        clipboard_mutation = $clipboardTargetEvidence
    }
    capture = [ordered]@{
        stable_focus = $stableCaptureEvidence
        focus_switch = $focusCaptureEvidence
        clipboard_mutation = $clipboardCaptureEvidence
    }
    cases = $caseResults
    failure_class = $null
    failure_stage = $null
    failure_detail = $null
}
$exitCode = 0
$currentStage = 'initialization'

try {
    $clipboardDirectory = Join-Path $runRoot 'clipboard-mutation'
    $clipboardBarrierDirectory = Join-Path $clipboardDirectory 'barriers'
    New-Item -ItemType Directory -Force -Path $clipboardBarrierDirectory | Out-Null
    $clipboardReceiptPath = Join-Path $evidenceDirectory 'clipboard-mutation.insertion.jsonl'
    $clipboardPlaybackPath = Join-Path $evidenceDirectory 'clipboard-mutation.playback.json'
    $clipboardApp = $null
    $clipboardTarget = $null
    $clipboardMutationWritten = $false
    $clipboardPreconditionEmpty = $false
    $clipboardPrimaryStage = $null
    $mutationMarker = $null
    try {
        $currentStage = 'clipboard_require_empty'
        if (-not [Verbatim.NativeSmoke.WindowInterop]::IsClipboardEmpty()) {
            throw 'Clipboard mutation smoke requires an empty clipboard to preserve existing physical-session content.'
        }
        $clipboardPreconditionEmpty = $true
        $currentStage = 'clipboard_start_app'
        $clipboardApp = Start-IsolatedApp -AppPath $appPath -CaseDirectory $clipboardDirectory -ModelDirectory $modelDirectory -ModelId $modelId -InputDeviceName $inputDeviceName -CaseName 'clipboard_mutation_during_paste_preserves_user_clipboard' -ReceiptPath $clipboardReceiptPath -StartupEvidence $clipboardStartupEvidence -BarrierDirectory $clipboardBarrierDirectory -BarrierStages 'after_clipboard_payload'
        $currentStage = 'clipboard_start_target'
        $clipboardTarget = Start-ControlledTarget -CaseDirectory $clipboardDirectory -Name 'clipboard-target' -TargetScriptPath $controlledTargetScriptPath -TargetEvidence $clipboardTargetEvidence
        $currentStage = 'clipboard_capture'
        $clipboardPlayback = Invoke-AudioCapture -AppPath $appPath -Target $clipboardTarget -WavPath $wavPath -OutputDeviceName $outputDeviceName -PlaybackScriptPath $playbackScriptPath -PlaybackReceiptPath $clipboardPlaybackPath -CaptureEvidence $clipboardCaptureEvidence
        $clipboardReadyPath = Join-Path $clipboardBarrierDirectory 'after_clipboard_payload.ready.json'
        $currentStage = 'clipboard_wait_barrier'
        Wait-Until -TimeoutMilliseconds 90000 -Description 'after-clipboard-payload barrier' -Condition { Test-Path -LiteralPath $clipboardReadyPath -PathType Leaf }
        $currentStage = 'clipboard_require_empty_before_mutation'
        if (-not [Verbatim.NativeSmoke.WindowInterop]::IsClipboardEmpty()) {
            throw 'Clipboard mutation smoke requires an empty clipboard to preserve existing physical-session content.'
        }
        $currentStage = 'clipboard_mutate'
        $mutationMarker = "verbatim-golden-clipboard-$([Guid]::NewGuid().ToString('N'))"
        Set-Clipboard -Value $mutationMarker
        $clipboardMutationWritten = $true
        [System.IO.File]::WriteAllText((Join-Path $clipboardBarrierDirectory 'after_clipboard_payload.continue'), '', [System.Text.UTF8Encoding]::new($false))
        $currentStage = 'clipboard_wait_receipt'
        $clipboardReceipt = Wait-InsertionReceipt -ReceiptPath $clipboardReceiptPath -CaseName 'clipboard mutation'
        $currentStage = 'clipboard_assert_receipt'
        Assert-InsertionReceipt -Receipt $clipboardReceipt -Attempted $true -Succeeded $false -TargetVerified $true -ErrorText 'clipboard changed before paste'
        $currentStage = 'clipboard_verify'
        $clipboardAfterMutation = ([string](Get-Clipboard -Raw)).TrimEnd("`r", "`n")
        $clipboardPreserved = $clipboardAfterMutation -eq $mutationMarker
        if (-not $clipboardPreserved) {
            throw 'Verbatim overwrote the newer synthetic clipboard value.'
        }
        $clipboardTargetState = Get-ControlledTargetState -Target $clipboardTarget
        if ($clipboardTargetState.Mutated) {
            throw 'Clipboard-mutation protection allowed a controlled target mutation.'
        }
        $caseResults += [ordered]@{
            name = 'clipboard_mutation_during_paste_preserves_user_clipboard'
            app_driven = $true
            fixture_rendered_to_virtual_output = [bool]$clipboardPlayback.success
            model_inference_reached_clipboard_write = $true
            insertion = [ordered]@{
                attempted = [bool]$clipboardReceipt.attempted
                succeeded = [bool]$clipboardReceipt.succeeded
                target_verified = [bool]$clipboardReceipt.target_verified
                error = $clipboardReceipt.error
            }
            synthetic_clipboard_mutation_preserved = $clipboardPreserved
            clipboard_precondition_empty = [bool]$clipboardPreconditionEmpty
            controlled_target_unchanged = -not [bool]$clipboardTargetState.Mutated
        }
    } catch {
        $clipboardPrimaryStage = $currentStage
        throw
    } finally {
        $cleanupFailure = $null
        $cleanupFailureStage = $null

        $currentStage = 'clipboard_cleanup_clipboard'
        try {
            if ($clipboardMutationWritten -and -not [string]::IsNullOrEmpty($mutationMarker)) {
                $currentClipboard = ([string](Get-Clipboard -Raw)).TrimEnd("`r", "`n")
                if ($currentClipboard -eq $mutationMarker) {
                    [void][Verbatim.NativeSmoke.WindowInterop]::ClearClipboardIfMatches($mutationMarker)
                }
            }
        } catch {
            $cleanupFailure = $_
            $cleanupFailureStage = $currentStage
        }

        $currentStage = 'clipboard_cleanup_target'
        try {
            if ($null -ne $clipboardTarget) { Close-ControlledTarget -Target $clipboardTarget }
        } catch {
            if ($null -eq $cleanupFailure) {
                $cleanupFailure = $_
                $cleanupFailureStage = $currentStage
            }
        }

        $currentStage = 'clipboard_cleanup_app'
        try {
            if ($null -ne $clipboardApp) { Stop-IsolatedApp -AppState $clipboardApp -ExpectedAppPath $appPath }
        } catch {
            if ($null -eq $cleanupFailure) {
                $cleanupFailure = $_
                $cleanupFailureStage = $currentStage
            }
        }

        $currentStage = 'clipboard_cleanup_case_work'
        try {
            Remove-CaseWork -CaseDirectory $clipboardDirectory
        } catch {
            if ($null -eq $cleanupFailure) {
                $cleanupFailure = $_
                $cleanupFailureStage = $currentStage
            }
        }

        if ($null -ne $cleanupFailure) {
            $currentStage = $cleanupFailureStage
            throw $cleanupFailure
        }
        if ($null -ne $clipboardPrimaryStage) {
            $currentStage = $clipboardPrimaryStage
        }
    }

    $stableDirectory = Join-Path $runRoot 'stable-focus'
    New-Item -ItemType Directory -Force -Path $stableDirectory | Out-Null
    $stableReceiptPath = Join-Path $evidenceDirectory 'stable-focus.insertion.jsonl'
    $stablePlaybackPath = Join-Path $evidenceDirectory 'stable-focus.playback.json'
    $stableApp = $null
    $stableTarget = $null
    try {
        $currentStage = 'stable_start_app'
        $stableApp = Start-IsolatedApp -AppPath $appPath -CaseDirectory $stableDirectory -ModelDirectory $modelDirectory -ModelId $modelId -InputDeviceName $inputDeviceName -CaseName 'stable_focus_inserts' -ReceiptPath $stableReceiptPath -StartupEvidence $stableStartupEvidence
        $currentStage = 'stable_start_target'
        $stableTarget = Start-ControlledTarget -CaseDirectory $stableDirectory -Name 'stable-focus-target' -TargetScriptPath $controlledTargetScriptPath -TargetEvidence $stableTargetEvidence
        $currentStage = 'stable_capture'
        $stablePlayback = Invoke-AudioCapture -AppPath $appPath -Target $stableTarget -WavPath $wavPath -OutputDeviceName $outputDeviceName -PlaybackScriptPath $playbackScriptPath -PlaybackReceiptPath $stablePlaybackPath -CaptureEvidence $stableCaptureEvidence
        $currentStage = 'stable_wait_receipt'
        $stableReceipt = Wait-InsertionReceipt -ReceiptPath $stableReceiptPath -CaseName 'stable focus'
        $currentStage = 'stable_assert_receipt'
        Assert-InsertionReceipt -Receipt $stableReceipt -Attempted $true -Succeeded $true -TargetVerified $true -ErrorText $null
        $currentStage = 'stable_verify_target'
        $stableTargetState = Get-ControlledTargetState -Target $stableTarget
        if (-not $stableTargetState.HasNonemptyInsertion) {
            throw 'Stable focus did not produce a nonempty app-driven insertion.'
        }
        $caseResults += [ordered]@{
            name = 'stable_focus_inserts'
            app_driven = $true
            fixture_rendered_to_virtual_output = [bool]$stablePlayback.success
            model_inference_completed = $true
            insertion = [ordered]@{
                attempted = [bool]$stableReceipt.attempted
                succeeded = [bool]$stableReceipt.succeeded
                target_verified = [bool]$stableReceipt.target_verified
                error = $stableReceipt.error
            }
            controlled_target_has_nonempty_insertion = [bool]$stableTargetState.HasNonemptyInsertion
        }
    } finally {
        try {
            if ($null -ne $stableTarget) { Close-ControlledTarget -Target $stableTarget }
        } finally {
            try {
                if ($null -ne $stableApp) { Stop-IsolatedApp -AppState $stableApp -ExpectedAppPath $appPath }
            } finally {
                Remove-CaseWork -CaseDirectory $stableDirectory
            }
        }
    }

    $focusDirectory = Join-Path $runRoot 'focus-switch'
    $focusBarrierDirectory = Join-Path $focusDirectory 'barriers'
    New-Item -ItemType Directory -Force -Path $focusBarrierDirectory | Out-Null
    $focusReceiptPath = Join-Path $evidenceDirectory 'focus-switch.insertion.jsonl'
    $focusPlaybackPath = Join-Path $evidenceDirectory 'focus-switch.playback.json'
    $focusApp = $null
    $originTarget = $null
    $replacementTarget = $null
    try {
        $currentStage = 'focus_start_app'
        $focusApp = Start-IsolatedApp -AppPath $appPath -CaseDirectory $focusDirectory -ModelDirectory $modelDirectory -ModelId $modelId -InputDeviceName $inputDeviceName -CaseName 'focus_switch_during_inference_blocks_insertion' -ReceiptPath $focusReceiptPath -StartupEvidence $focusStartupEvidence -BarrierDirectory $focusBarrierDirectory -BarrierStages 'before_insertion'
        $currentStage = 'focus_start_targets'
        $originTarget = Start-ControlledTarget -CaseDirectory $focusDirectory -Name 'origin-target' -TargetScriptPath $controlledTargetScriptPath -TargetEvidence $focusOriginTargetEvidence
        $replacementTarget = Start-ControlledTarget -CaseDirectory $focusDirectory -Name 'replacement-target' -TargetScriptPath $controlledTargetScriptPath -TargetEvidence $focusReplacementTargetEvidence
        Set-ControlledTargetFocus -Target $originTarget -Label 'origin target' -TargetEvidence $originTarget.Evidence
        $currentStage = 'focus_capture'
        $focusPlayback = Invoke-AudioCapture -AppPath $appPath -Target $originTarget -WavPath $wavPath -OutputDeviceName $outputDeviceName -PlaybackScriptPath $playbackScriptPath -PlaybackReceiptPath $focusPlaybackPath -CaptureEvidence $focusCaptureEvidence
        $focusReadyPath = Join-Path $focusBarrierDirectory 'before_insertion.ready.json'
        $currentStage = 'focus_wait_barrier'
        Wait-Until -TimeoutMilliseconds 90000 -Description 'before-insertion barrier' -Condition { Test-Path -LiteralPath $focusReadyPath -PathType Leaf }
        $currentStage = 'focus_switch_target'
        Set-ControlledTargetFocus -Target $replacementTarget -Label 'replacement target' -TargetEvidence $replacementTarget.Evidence
        [System.IO.File]::WriteAllText((Join-Path $focusBarrierDirectory 'before_insertion.continue'), '', [System.Text.UTF8Encoding]::new($false))
        $currentStage = 'focus_wait_receipt'
        $focusReceipt = Wait-InsertionReceipt -ReceiptPath $focusReceiptPath -CaseName 'focus switch'
        $currentStage = 'focus_assert_receipt'
        Assert-InsertionReceipt -Receipt $focusReceipt -Attempted $false -Succeeded $false -TargetVerified $false -ErrorText 'target changed before insertion'
        $currentStage = 'focus_verify_targets'
        $originState = Get-ControlledTargetState -Target $originTarget
        $replacementState = Get-ControlledTargetState -Target $replacementTarget
        if ($originState.Mutated -or $replacementState.Mutated) {
            throw 'Focus-switch protection allowed a controlled target mutation.'
        }
        $caseResults += [ordered]@{
            name = 'focus_switch_during_inference_blocks_insertion'
            app_driven = $true
            fixture_rendered_to_virtual_output = [bool]$focusPlayback.success
            model_inference_reached_insertion = $true
            insertion = [ordered]@{
                attempted = [bool]$focusReceipt.attempted
                succeeded = [bool]$focusReceipt.succeeded
                target_verified = [bool]$focusReceipt.target_verified
                error = $focusReceipt.error
            }
            origin_target_unchanged = -not [bool]$originState.Mutated
            replacement_target_unchanged = -not [bool]$replacementState.Mutated
        }
    } finally {
        try {
            if ($null -ne $originTarget) { Close-ControlledTarget -Target $originTarget }
        } finally {
            try {
                if ($null -ne $replacementTarget) { Close-ControlledTarget -Target $replacementTarget }
            } finally {
                try {
                    if ($null -ne $focusApp) { Stop-IsolatedApp -AppState $focusApp -ExpectedAppPath $appPath }
                } finally {
                    Remove-CaseWork -CaseDirectory $focusDirectory
                }
            }
        }
    }

}
catch {
    $report.failure_class = 'golden_dictation_failed'
    $report.failure_stage = $currentStage
    $report.failure_detail = Get-SafeFailureDetail -ErrorRecord $_
    $exitCode = 1
    Write-Error $_.Exception.Message
}
finally {
    $report.cases = $caseResults
    $reportPath = Join-Path $evidenceDirectory 'golden-dictation.json'
    [System.IO.File]::WriteAllText(
        $reportPath,
        (($report | ConvertTo-Json -Depth 12) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    Remove-Item -LiteralPath $runRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if ($exitCode -ne 0) {
    exit $exitCode
}
