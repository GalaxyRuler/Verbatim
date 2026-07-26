[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$Name,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$StatePath,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$StopPath,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$FocusPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($path in @($StatePath, $StopPath, $FocusPath)) {
    if (-not [System.IO.Path]::IsPathRooted($path)) {
        throw 'Controlled-target paths must be absolute.'
    }
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$stateParent = Split-Path -Parent $StatePath
New-Item -ItemType Directory -Force -Path $stateParent | Out-Null
Remove-Item -LiteralPath $StopPath -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $FocusPath -Force -ErrorAction SilentlyContinue

$form = New-Object System.Windows.Forms.Form
$form.Text = "Verbatim native smoke - $Name"
$form.Width = 720
$form.Height = 360
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::CenterScreen
$form.ShowInTaskbar = $true

$textBox = New-Object System.Windows.Forms.TextBox
$textBox.Multiline = $true
$textBox.AcceptsReturn = $true
$textBox.AcceptsTab = $true
$textBox.ScrollBars = [System.Windows.Forms.ScrollBars]::Vertical
$textBox.Dock = [System.Windows.Forms.DockStyle]::Fill
$textBox.Text = "verbatim-golden-$Name-$([Guid]::NewGuid().ToString('N'))"
$form.Controls.Add($textBox)
$form.ActiveControl = $textBox
$initialLength = [int]$textBox.TextLength
$script:stateSequence = 0

function Focus-ControlledTextBox {
    $form.ActiveControl = $textBox
    $textBox.Focus()
    $textBox.SelectionStart = $textBox.TextLength
    $textBox.SelectionLength = 0
}

function Write-ControlledTargetState {
    $script:stateSequence += 1
    $state = [ordered]@{
        schema_version = 1
        process_id = $PID
        sequence = $script:stateSequence
        window_handle = [int64]$form.Handle.ToInt64()
        initial_length = $initialLength
        current_length = [int]$textBox.TextLength
        visible = [bool]$form.Visible
        form_contains_focus = [bool]$form.ContainsFocus
        text_box_focused = [bool]$textBox.Focused
    }
    $temporaryPath = "$StatePath.$PID.tmp"
    [System.IO.File]::WriteAllText(
        $temporaryPath,
        (($state | ConvertTo-Json -Compress) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::Copy($temporaryPath, $StatePath, $true)
    Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction SilentlyContinue
}

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 50
$timer.add_Tick({
        if (Test-Path -LiteralPath $StopPath -PathType Leaf) {
            $timer.Stop()
            $form.Close()
            return
        }
        if (Test-Path -LiteralPath $FocusPath -PathType Leaf) {
            Remove-Item -LiteralPath $FocusPath -Force -ErrorAction SilentlyContinue
            $form.Activate()
            Focus-ControlledTextBox
        }
        Write-ControlledTargetState
    })
$form.add_Shown({
        Focus-ControlledTextBox
        Write-ControlledTargetState
        $timer.Start()
    })
$form.add_Activated({
        Focus-ControlledTextBox
        Write-ControlledTargetState
    })
$form.add_FormClosed({
        $timer.Stop()
    })

[System.Windows.Forms.Application]::Run($form)
