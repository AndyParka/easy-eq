param(
    [switch] $Release, [switch] $Restore,
    [string] $LibraryPath, [string] $InitializerPath,
    [string] $UserSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$apo = Get-ItemProperty -LiteralPath 'HKLM:/SOFTWARE/EqualizerAPO'
$configDirectory = [IO.Path]::GetFullPath($apo.ConfigPath)
$rootConfig = Join-Path $configDirectory 'config.txt'
$ownedDirectory = Join-Path $configDirectory 'Peace2'
$ownedFragment = Join-Path $ownedDirectory 'config.txt'
$original = [IO.File]::ReadAllText($rootConfig)
$marker = '# Peace 2 audio bridge (managed)'
function Write-AtomicText([string] $path, [string] $text) {
    $temporary = $path + '.peace2.tmp'
    [IO.File]::WriteAllText($temporary, $text, [Text.UTF8Encoding]::new($false))
    if ([IO.File]::Exists($path)) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
    else { [IO.File]::Move($temporary, $path) }
}
if ($Restore) {
    $clean = [regex]::Replace($original, '(?m)^# Peace 2 audio bridge \(managed\)\r?\nInclude: Peace2\\config.txt\r?\n?', '')
    Write-AtomicText $rootConfig $clean
    Write-Output 'Removed the Easy EQ include. Existing APO settings are preserved. Stop Easy EQ to revoke its lease.'
    exit 0
}
$build = if ($Release) { 'release' } else { 'debug' }
$library = if ($LibraryPath) { [IO.Path]::GetFullPath($LibraryPath) } else { Join-Path $projectRoot "crates/easy-eq-vst/target/$build/peace2_audio.dll" }
$initializer = if ($InitializerPath) { [IO.Path]::GetFullPath($InitializerPath) } else { Join-Path $projectRoot 'crates/easy-eq-bridge/target/debug/init-control.exe' }
if (!(Test-Path -LiteralPath $library) -or !(Test-Path -LiteralPath $initializer)) { throw 'Build the audio DLL and init-control helper before installation.' }
$libraryHash = (Get-FileHash -LiteralPath $library -Algorithm SHA256).Hash
$libraryName = 'peace2_audio_' + $libraryHash.Substring(0, 12).ToLowerInvariant() + '.dll'
$postMix = '{EC1CC9CE-FAED-4822-828A-82A81A6F018F}'
$installed = @(Get-ChildItem -LiteralPath 'HKLM:/SOFTWARE/Microsoft/Windows/CurrentVersion/MMDevices/Audio/Render' | ForEach-Object {
    $effectsPath = Join-Path $_.PSPath 'FxProperties'
    if (Test-Path -LiteralPath $effectsPath) {
        $effects = Get-ItemProperty -LiteralPath $effectsPath
        if (@($effects.PSObject.Properties | Where-Object { $_.Name -match ',(2|6|7|14|15)$' -and @($_.Value) -contains $postMix }).Count -gt 0) { $_.PSChildName }
    }
})
if (!$installed.Count) { throw 'No outputs have an Equalizer APO final effects stage. Enable an output in APO Device Selector first.' }
[IO.Directory]::CreateDirectory($ownedDirectory) | Out-Null
$backup = Join-Path $ownedDirectory ('config.before-peace2-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '.txt')
if (!$original.Contains($marker)) { [IO.File]::WriteAllText($backup, $original, [Text.UTF8Encoding]::new($false)) }
$stateRoot = Join-Path $env:ProgramData 'Peace2'
[IO.Directory]::CreateDirectory($stateRoot) | Out-Null
$targetUserSid = [Security.Principal.SecurityIdentifier]::new($UserSid)
$serviceSid = [Security.Principal.SecurityIdentifier]::new('S-1-5-19')
$lines = [Collections.Generic.List[string]]::new()
$lines.Add('# Easy EQ: final output stage only; L/R remain separate.')
$lines.Add('Stage: post-mix')
foreach ($guid in $installed) {
    $deviceFolder = Join-Path $ownedDirectory $guid
    [IO.Directory]::CreateDirectory($deviceFolder) | Out-Null
    # Use a new path when the component changes. Windows may still have the
    # previous DLL loaded; APO reloads the owned fragment without a service restart.
    $destination = Join-Path $deviceFolder $libraryName
    if (!(Test-Path -LiteralPath $destination)) { Copy-Item -LiteralPath $library -Destination $destination }
    elseif ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $libraryHash) { throw 'Installed audio component checksum does not match.' }
    $stateFolder = Join-Path $stateRoot $guid
    [IO.Directory]::CreateDirectory($stateFolder) | Out-Null
    $stateFile = Join-Path $stateFolder 'control.bin'
    if (!(Test-Path -LiteralPath $stateFile)) { & $initializer $stateFile; if ($LASTEXITCODE) { throw 'Could not initialize the audio control mapping.' } }
    # Grant only this user and the Windows audio service write access. Control
    # files are outside APO's watched config tree to avoid configuration reloads.
    $acl = Get-Acl -LiteralPath $stateFile
    $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($targetUserSid, 'Modify', 'Allow'))
    $acl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($serviceSid, 'Modify', 'Allow'))
    Set-Acl -LiteralPath $stateFile -AclObject $acl
    $spectrumFile = Join-Path $stateFolder 'spectrum.bin'
    if (!(Test-Path -LiteralPath $spectrumFile)) { & $initializer --spectrum $spectrumFile; if ($LASTEXITCODE) { throw 'Could not initialize the spectrum mapping.' } }
    $spectrumAcl = Get-Acl -LiteralPath $spectrumFile
    $spectrumAcl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($targetUserSid, 'Modify', 'Allow'))
    $spectrumAcl.SetAccessRule([Security.AccessControl.FileSystemAccessRule]::new($serviceSid, 'Modify', 'Allow'))
    Set-Acl -LiteralPath $spectrumFile -AclObject $spectrumAcl
    $lines.Add("Device: $guid")
    $lines.Add('Channel: L R')
    $lines.Add('VSTPlugin: Library "' + $destination + '"')
}
$lines.Add('Device: all')
$lines.Add('Channel: all')
Write-AtomicText $ownedFragment (($lines -join "`r`n") + "`r`n")
if (!$original.Contains($marker)) {
    Write-AtomicText $rootConfig ($original.TrimEnd() + "`r`n" + $marker + "`r`nInclude: Peace2\config.txt`r`n")
}
Write-Output ("Installed the Easy EQ audio bridge for {0} APO outputs. No device registration or audio-service restart was performed." -f $installed.Count)
Write-Output ("Restore command: .\scripts\install-audio.ps1 -Restore")
