param([switch] $SkipBuild, [string] $Iscc)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    if (!$SkipBuild) {
        foreach ($manifest in @('easy-eq-desktop','easy-eq-vst','easy-eq-bridge')) {
            & (Join-Path $PSScriptRoot 'rust.ps1') build --release --locked --manifest-path "crates/$manifest/Cargo.toml"
            if ($LASTEXITCODE) { throw "Build failed: $manifest" }
        }
    }
    $version = [regex]::Match([IO.File]::ReadAllText((Join-Path $projectRoot 'crates/easy-eq-desktop/Cargo.toml')), '(?m)^version = "([^"]+)"').Groups[1].Value
    if (!$version) { throw 'Cannot determine release version.' }
    $payload = Join-Path $projectRoot 'artifacts/installer-payload'
    $output = Join-Path $projectRoot 'artifacts/release'
    [IO.Directory]::CreateDirectory($payload) | Out-Null
    [IO.Directory]::CreateDirectory($output) | Out-Null
    $dependencies = @(
        @{ File='EqualizerAPO-x64-1.4.2.exe'; Hash='7403BE7427BBE1936A40DDED082829B6E217FC4F5990FEE5CBA501F0AE055AFA'; Magic='MZ' },
        @{ File='EqualizerAPO-src-1.4.2.zip'; Hash='BACD5C78BE71F3AAAB010684AB39F0E5C1BBB45FCD75CBB92C103A145A731DC4'; Magic='PK' }
    )
    foreach ($dependency in $dependencies) {
        $download = Join-Path $projectRoot ('.tools/downloads/' + $dependency.File)
        if (!(Test-Path -LiteralPath $download)) {
            [IO.Directory]::CreateDirectory((Split-Path -Parent $download)) | Out-Null
            $url = 'https://sourceforge.net/projects/equalizerapo/files/1.4.2/' + $dependency.File + '/download'
            $page = (Invoke-WebRequest -Uri $url -UseBasicParsing).Content
            $link = [Net.WebUtility]::HtmlDecode([regex]::Match($page, 'http-equiv="refresh" content="5; url=([^"]+)').Groups[1].Value)
            if (!$link.StartsWith('https://downloads.sourceforge.net/project/equalizerapo/')) { throw 'Unexpected dependency download URL.' }
            Invoke-WebRequest -Uri $link -OutFile $download -UseBasicParsing
        }
        $bytes = [IO.File]::ReadAllBytes($download)
        if ([Text.Encoding]::ASCII.GetString($bytes,0,2) -ne $dependency.Magic -or (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ne $dependency.Hash) { throw 'Equalizer APO dependency checksum or executable format failed.' }
        if ($dependency.Magic -eq 'MZ') { Copy-Item -LiteralPath $download -Destination $payload }
        else { Copy-Item -LiteralPath $download -Destination $output }
    }
    Copy-Item -LiteralPath 'crates/easy-eq-desktop/target/release/easy-eq.exe' -Destination (Join-Path $payload 'easy-eq.exe')
    Copy-Item -LiteralPath 'crates/easy-eq-vst/target/release/peace2_audio.dll','crates/easy-eq-bridge/target/release/init-control.exe','scripts/install-audio.ps1','scripts/setup-audio.ps1','LICENSE','README.md','docs/THIRD-PARTY-NOTICES.md' -Destination $payload
    & (Join-Path $PSScriptRoot 'collect-licenses.ps1') -Destination (Join-Path $payload 'third-party-licenses')
    Copy-Item -LiteralPath ("docs/release-notes-$version.md") -Destination $output
    if (!$Iscc) {
        $candidates = @((Join-Path $projectRoot '.tools/inno/ISCC.exe'), 'C:/Program Files (x86)/Inno Setup 6/ISCC.exe', 'C:/Program Files/Inno Setup 7/ISCC.exe')
        $Iscc = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (!$Iscc) { $Iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source }
    }
    if (!$Iscc) { throw 'Install Inno Setup 6 or 7, or pass -Iscc with its compiler path.' }
    & $Iscc "/DVersion=$version" "/DPayloadDir=$payload" "/DOutputDir=$output" 'installer/easy-eq.iss'
    if ($LASTEXITCODE) { throw 'Installer compilation failed.' }
    $installer = Join-Path $output "Easy-EQ-$version-windows-x64-setup.exe"
    $sums = @(Get-ChildItem -LiteralPath $output -File | Where-Object { $_.Extension -in @('.exe','.zip') } | ForEach-Object { (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $_.Name })
    [IO.File]::WriteAllLines((Join-Path $output 'SHA256SUMS.txt'), $sums, [Text.UTF8Encoding]::new($false))
    Write-Output $installer
} finally { Pop-Location }
