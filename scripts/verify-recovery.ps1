param([string] $Executable = (Join-Path $PSScriptRoot '../crates/easy-eq-desktop/target/release/easy-eq.exe'))
$ErrorActionPreference = 'Stop'
$Executable = [IO.Path]::GetFullPath($Executable)
$directory = Join-Path $env:APPDATA 'technology.parkinson.peace2'
$workspace = Join-Path $directory 'workspace.json'
$statusPath = Join-Path $directory 'supervisor.json'
$saved = if (Test-Path -LiteralPath $workspace) { [IO.File]::ReadAllText($workspace) } else { $null }
$results = [Collections.Generic.List[string]]::new()
if (-not ('EasyEqProcessPause' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class EasyEqProcessPause {
    [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr process);
    [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr process);
}
'@
}
function Send-Command([string] $flag) {
    $command = Start-Process -FilePath $Executable -ArgumentList $flag -WindowStyle Hidden -Wait -PassThru
    if ($command.ExitCode) { throw "Command failed: $flag" }
}
function Wait-Worker([int] $Previous = 0) {
    $deadline = [DateTime]::UtcNow.AddSeconds(25)
    do {
        if (Test-Path -LiteralPath $statusPath) {
            try {
                $status = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json
                $worker = Get-Process -Id $status.workerPid -ErrorAction SilentlyContinue
                if ($worker -and $worker.Id -ne $Previous) { return $status }
            } catch {}
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Watchdog did not start a fresh editor.'
}
$supervisor = $null
try {
    Send-Command '--quit'
    Start-Sleep -Seconds 2
    $supervisor = Start-Process -FilePath $Executable -ArgumentList '--tray' -WindowStyle Hidden -PassThru
    $first = Wait-Worker
    Start-Sleep -Seconds 3
    if ($first.supervisorPid -ne $supervisor.Id) { throw 'A different watchdog owns the editor.' }
    Send-Command '--tray'
    $second = Wait-Worker
    if ($second.workerPid -ne $first.workerPid) { throw 'Duplicate launch replaced the editor.' }
    $results.Add('PASS: hidden startup and duplicate launch preserve one supervised editor.')
    Send-Command '--off'
    Start-Sleep -Milliseconds 500
    $worker = Get-Process -Id $first.workerPid
    if ($worker.Path -ne $Executable) { throw 'Refusing to stop an unrelated process.' }
    Stop-Process -Id $worker.Id -Force
    $restarted = Wait-Worker $worker.Id
    Start-Sleep -Seconds 3
    if ((Get-Content -LiteralPath $workspace -Raw | ConvertFrom-Json).enabled) { throw 'Crash recovery changed intentional Off state.' }
    $results.Add('PASS: forced editor crash restarts in the tray and preserves Off.')
    $resume = [Threading.EventWaitHandle]::OpenExisting('Local\Peace2.Native.Supervisor.Resume')
    try { $resume.Set() | Out-Null } finally { $resume.Dispose() }
    Start-Sleep -Seconds 3
    if ((Get-Content -LiteralPath $directory/native.log -Raw) -notmatch 'Audio control resumed: reopened bridge mappings') { throw 'Resume did not reopen audio mappings.' }
    $results.Add('PASS: resume notification refreshes device discovery and control mappings.')
    $worker = Get-Process -Id $restarted.workerPid
    if ($worker.Path -ne $Executable) { throw 'Refusing to suspend an unrelated process.' }
    try {
        if ([EasyEqProcessPause]::NtSuspendProcess($worker.Handle)) { throw 'Could not pause editor.' }
        if ([EasyEqProcessPause]::NtSuspendProcess($supervisor.Handle)) { throw 'Could not pause watchdog.' }
        Start-Sleep -Seconds 6
    } finally {
        $null = [EasyEqProcessPause]::NtResumeProcess($worker.Handle)
        $null = [EasyEqProcessPause]::NtResumeProcess($supervisor.Handle)
    }
    Start-Sleep -Seconds 3
    if ((Wait-Worker).workerPid -ne $worker.Id) { throw 'A resume scheduling gap caused an unnecessary restart.' }
    $results.Add('PASS: pausing both processes simulates a resume gap without a false watchdog restart.')
    try {
        if ([EasyEqProcessPause]::NtSuspendProcess($worker.Handle)) { throw 'Could not simulate a stalled editor.' }
        $restarted = Wait-Worker $worker.Id
    } finally {
        if (!$worker.HasExited) { $null = [EasyEqProcessPause]::NtResumeProcess($worker.Handle) }
    }
    $results.Add('PASS: a stalled editor heartbeat is detected and a fresh editor starts automatically.')
    Send-Command '--quit'
    if (!$supervisor.WaitForExit(8000)) { throw 'Explicit Quit did not stop the watchdog.' }
    Start-Sleep -Seconds 2
    if (Get-Process -Id $restarted.workerPid -ErrorAction SilentlyContinue) { throw 'Quit left an editor running.' }
    $results.Add('PASS: explicit Quit stops both processes without a restart.')
    $results | Write-Output
} finally {
    Send-Command '--quit'
    if ($supervisor) { $null = $supervisor.WaitForExit(8000) }
    if ($null -ne $saved) { [IO.File]::WriteAllText($workspace, $saved, [Text.UTF8Encoding]::new($false)) }
    [IO.Directory]::CreateDirectory((Join-Path $PSScriptRoot '../artifacts')) | Out-Null
    [IO.File]::WriteAllLines((Join-Path $PSScriptRoot '../artifacts/recovery-validation.txt'), $results)
}
