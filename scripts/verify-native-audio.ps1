param([switch] $LeaveClosed)
# Verify the native editor and live audiodg callbacks using silent playback.
# Restore all saved profiles, routing, startup and on/off preferences.
$ErrorActionPreference = 'Stop'
if (-not ('Peace2WindowVisibility' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class Peace2WindowVisibility {
    delegate bool Callback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    public static IntPtr Editor(int process) {
        IntPtr editor = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == process) {
                var title = new StringBuilder(256); GetWindowText(window, title, 256);
                if (title.ToString() == "Easy EQ") { editor = window; return false; }
            }
            return true;
        }, IntPtr.Zero);
        return editor;
    }
}
'@
}
$projectRoot = Split-Path -Parent $PSScriptRoot
$executable = [IO.Path]::GetFullPath((Join-Path $projectRoot 'crates/easy-eq-desktop/target/debug/easy-eq.exe'))
$checker = [IO.Path]::GetFullPath((Join-Path $projectRoot 'crates/easy-eq-devices/target/debug/exercise-output.exe'))
$inspector = [IO.Path]::GetFullPath((Join-Path $projectRoot 'crates/easy-eq-bridge/target/debug/inspect-control.exe'))
$workspacePath = Join-Path $env:APPDATA 'technology.parkinson.peace2/workspace.json'
$nativeLog = Join-Path $env:APPDATA 'technology.parkinson.peace2/native.log'
$savedRaw = [IO.File]::ReadAllText($workspacePath)
$saved = $savedRaw | ConvertFrom-Json
$guid = [regex]::Match($saved.settings.lastOutputId, '\{[0-9a-f-]{36}\}$', 'IgnoreCase').Value
if (!$guid) { throw 'Open the desktop app once to identify the default output.' }
$control = Join-Path $env:ProgramData "Peace2/$guid/control.bin"
$script:auditRun = 0
$script:auditPreview = $null
$checks = [Collections.Generic.List[string]]::new()
$checkResults = Join-Path $projectRoot 'artifacts/native-audio-validation.txt'
function Start-Preview([bool] $tray) {
    $script:auditRun++
    $parameters = @{FilePath=$executable; WindowStyle='Hidden'; PassThru=$true}
    if ($tray) { $parameters.ArgumentList = @('--tray') }
    $script:auditPreview = Start-Process @parameters
}
function Send-Command([string] $flag) {
    $secondary = Start-Process -FilePath $executable -ArgumentList $flag -WindowStyle Hidden -Wait -PassThru
    if ($secondary.ExitCode) { throw "Native command failed: $flag" }
}
function Assert-Callback([bool] $processing, [string] $label) {
    $status = (& $inspector $control --response) -join ' '
    if ($LASTEXITCODE) { throw "Could not inspect callback: $label" }
    $expected = if ($processing) {'processing=1'} else {'processing=0'}
    $age = [int64][regex]::Match($status, 'callback_age_ms=(\d+)').Groups[1].Value
    $published = [regex]::Match($status, 'published=(\d+)').Groups[1].Value
    $applied = [regex]::Match($status, 'applied=(\d+)').Groups[1].Value
    if ($status -notmatch $expected -or $age -gt 150 -or $published -ne $applied) { throw "Live callback check failed: $label : $status" }
    $checks.Add("$label : $status")
    Write-Host "$label : $status"
    return $status
}
function Start-Silent {
    $parameters = @{FilePath=$checker; ArgumentList='15'; WindowStyle='Hidden'; PassThru=$true
        RedirectStandardOutput=(Join-Path $projectRoot "artifacts/audio-audit-silence-$script:auditRun.txt")
        RedirectStandardError=(Join-Path $projectRoot "artifacts/audio-audit-silence-$script:auditRun.err.txt")}
    Start-Process @parameters
}
$silent = $null
try {
    $existing = @(Get-Process -Name easy-eq -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $executable })
    if ($existing.Count) { $script:auditPreview = $existing[0] } else { Start-Preview $false }
    Start-Sleep -Seconds 2
    $silent = Start-Silent
    Start-Sleep -Milliseconds 400
    Send-Command '--test-tray-on'
    Start-Sleep -Milliseconds 350
    $null = Assert-Callback $true 'Enabled'
    Send-Command '--test-controls'
    Start-Sleep -Milliseconds 450
    $status = Assert-Callback $true 'Editor shared EQ, gain, balance and per-ear EQ'
    $left = [double]::Parse([regex]::Match($status, 'left_1000_db=(-?[\d.]+)').Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
    $right = [double]::Parse([regex]::Match($status, 'right_1000_db=(-?[\d.]+)').Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
    if ([Math]::Abs($left + 17.498775) -gt 0.001 -or [Math]::Abs($right + 9) -gt 0.001 -or $status -notmatch 'cross_lr=0 cross_rl=0') {
        throw 'Editor controls did not publish the expected independent-ear graph.'
    }
    Send-Command '--test-restore-controls'
    Start-Sleep -Milliseconds 350
    $null = Assert-Callback $true 'Original profile restored'
    # Send real WM_CLOSE to exercise the actual window-close event.
    $script:auditPreview.Refresh()
    if (!$script:auditPreview.CloseMainWindow()) { throw 'The native editor could not receive WM_CLOSE.' }
    Start-Sleep -Milliseconds 500
    $null = Assert-Callback $true 'After actual window close'
    $script:auditPreview.Refresh()
    if ($script:auditPreview.HasExited) { throw 'Closing the editor stopped the host.' }
    if ((Get-Content -LiteralPath $nativeLog -Raw) -notmatch 'Editor close intercepted.*Editor visible: false') { throw 'The editor did not hide.' }
    Send-Command '--test-tray-off'
    Start-Sleep -Milliseconds 350
    $null = Assert-Callback $false 'Explicit Off'
    Send-Command '--test-tray-quit'
    if (!$script:auditPreview.WaitForExit(3000)) { throw 'Explicit Quit did not exit the host.' }
    Start-Preview $true
    Start-Sleep -Seconds 2
    $null = Assert-Callback $false 'Tray startup restores Off'
    $script:auditPreview.Refresh()
    $editorWindow = [Peace2WindowVisibility]::Editor($script:auditPreview.Id)
    if ($editorWindow -eq [IntPtr]::Zero) { throw 'Tray startup did not create the editor window.' }
    if ([Peace2WindowVisibility]::IsWindowVisible($editorWindow)) { throw 'Tray startup displayed the editor.' }
    if (([IO.File]::ReadAllText($workspacePath) | ConvertFrom-Json).enabled) { throw 'Startup changed the saved Off state.' }
    Send-Command '--tray'
    Start-Sleep -Milliseconds 200
    if (@(Get-Process -Name easy-eq -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $executable }).Count -ne 1) {
        throw 'Second startup created another native host.'
    }
    Send-Command '--test-tray-on'
    Start-Sleep -Milliseconds 350
    $null = Assert-Callback $true 'Re-enabled'
    Send-Command '--test-tray-quit'
    if (!$script:auditPreview.WaitForExit(3000)) { throw 'Quit while enabled did not exit.' }
    $status = Assert-Callback $false 'Quit while enabled'
    if ($status -notmatch 'lease_live=false') { throw 'Quit did not revoke the lease.' }
    if ($silent -and !$silent.HasExited) { $null = $silent.WaitForExit(16000) }
    Start-Preview $true
    Start-Sleep -Seconds 2
    $silent = Start-Silent
    Send-Command '--test-tray-on'
    Start-Sleep -Milliseconds 450
    $null = Assert-Callback $true 'Before host kill'
    $script:auditPreview.Refresh()
    if ($script:auditPreview.Path -ne $executable) { throw 'Refusing to terminate an unrelated process.' }
    Stop-Process -Id $script:auditPreview.Id -Force
    Start-Sleep -Milliseconds 1750
    $status = Assert-Callback $false 'Forced host kill expires lease'
    if ($status -notmatch 'lease_live=false') { throw 'Killed host retained a live lease.' }
    $checks.Add('PASS: editor controls, actual window close, Off, Quit, startup, single instance and forced kill.')
    Write-Output $checks[$checks.Count - 1]
} finally {
    $running = @(Get-Process -Name easy-eq -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $executable })
    if ($running.Count) {
        Send-Command '--test-restore-controls'
        Send-Command '--test-tray-quit'
        $null = $running[0].WaitForExit(3000)
    }
    # Restore the exact document, including a fixture interrupted by any failure.
    [IO.File]::WriteAllText($workspacePath, $savedRaw, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllLines($checkResults, $checks)
    if (!$LeaveClosed) { Start-Preview $false }
    if ($silent -and !$silent.HasExited) { $null = $silent.WaitForExit(16000) }
}
