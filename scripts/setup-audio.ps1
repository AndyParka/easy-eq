param(
    [switch] $Restore, [switch] $Elevated,
    [string] $UserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value,
    [string] $LogPath = (Join-Path $env:LOCALAPPDATA 'Easy EQ/setup-audio.log')
)
$ErrorActionPreference = 'Stop'
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    if ($Elevated) { throw 'Administrator access is needed to set up Windows audio effects.' }
    $arguments = '-NoProfile -ExecutionPolicy Bypass -File "' + $PSCommandPath + '" -Elevated -UserSid "' + $UserSid + '" -LogPath "' + $LogPath + '"'
    if ($Restore) { $arguments += ' -Restore' }
    try {
        $process = Start-Process -FilePath "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe" -Verb RunAs -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
        exit $process.ExitCode
    } catch { Write-Error "Audio setup was cancelled or could not be elevated: $_"; exit 1 }
}
[IO.Directory]::CreateDirectory((Split-Path -Parent $LogPath)) | Out-Null
Start-Transcript -Path $LogPath -Append | Out-Null
try {
    $apoKey = 'HKLM:/SOFTWARE/EqualizerAPO'
    if (!(Test-Path -LiteralPath $apoKey)) {
        if ($Restore) { exit 0 }
        $dependency = Join-Path $PSScriptRoot 'EqualizerAPO-x64-1.4.2.exe'
        if (!(Test-Path -LiteralPath $dependency)) { throw 'The bundled Equalizer APO installer is missing. Reinstall Easy EQ.' }
        # Device selection is deliberately interactive: only the owner knows
        # which outputs should receive system effects. Do not guess or replace
        # device-driver APO registrations ourselves.
        $process = Start-Process -FilePath $dependency -Wait -PassThru
        if ($process.ExitCode -notin @(0,3010)) { throw "Equalizer APO installation failed ($($process.ExitCode))." }
        if (!(Test-Path -LiteralPath $apoKey)) { throw 'Complete Equalizer APO installation, then run Set up audio again.' }
    }
    if (!$Restore) {
        $postMix = '{EC1CC9CE-FAED-4822-828A-82A81A6F018F}'
        $registered = @(Get-ChildItem -LiteralPath 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/MMDevices/Audio/Render' | Where-Object {
            $fx = Join-Path $_.PSPath 'FxProperties'
            (Test-Path -LiteralPath $fx) -and @((Get-ItemProperty -LiteralPath $fx).PSObject.Properties | Where-Object { $_.Name -match ',(2|6|7|14|15)$' -and @($_.Value) -contains $postMix }).Count -gt 0
        })
        if (!$registered.Count) {
            $apo = Get-ItemProperty -LiteralPath $apoKey
            $selector = Join-Path $apo.InstallPath 'DeviceSelector.exe'
            $process = Start-Process -FilePath $selector -WorkingDirectory $apo.InstallPath -Wait -PassThru
            if ($process.ExitCode) { throw 'Select your playback output in Equalizer APO Device Selector, then retry audio setup.' }
        }
    }
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'install-audio.ps1'),
        '-LibraryPath', (Join-Path $PSScriptRoot 'peace2_audio.dll'),
        '-InitializerPath', (Join-Path $PSScriptRoot 'init-control.exe'), '-UserSid', $UserSid)
    if ($Restore) { $arguments += '-Restore' }
    # Run in a child scope because the development-compatible script uses exit.
    & "$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe" @arguments
    if ($LASTEXITCODE) { throw "Audio bridge setup failed ($LASTEXITCODE). See $LogPath" }
} catch { Write-Error $_; exit 1 }
finally { Stop-Transcript | Out-Null }
