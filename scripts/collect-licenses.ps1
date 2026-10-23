param([Parameter(Mandatory=$true)][string] $Destination)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
[IO.Directory]::CreateDirectory($Destination) | Out-Null
$metadata = (& (Join-Path $PSScriptRoot 'rust.ps1') metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc --manifest-path (Join-Path $projectRoot 'crates/easy-eq-desktop/Cargo.toml')) -join "`n"
if ($LASTEXITCODE) { throw 'Could not collect dependency metadata.' }
$packages = ($metadata | ConvertFrom-Json).packages | Where-Object { $_.source }
$index = [Collections.Generic.List[string]]::new()
foreach ($package in $packages) {
    $directory = Split-Path -Parent $package.manifest_path
    $target = Join-Path $Destination ($package.name + '-' + $package.version)
    [IO.Directory]::CreateDirectory($target) | Out-Null
    $index.Add("$($package.name) $($package.version): $($package.license)")
    # Include embedded fonts' licenses as well as the crate's root notices.
    $licenses = @(Get-ChildItem -LiteralPath $directory -File -Recurse | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|OFL|UFL|COPYRIGHT)' })
    if (!$licenses.Count -and $package.license_file) { $licenses = @(Get-Item -LiteralPath (Join-Path $directory $package.license_file)) }
    foreach ($license in $licenses) {
        $relative = $license.FullName.Substring($directory.Length).TrimStart('\','/')
        $output = Join-Path $target $relative
        [IO.Directory]::CreateDirectory((Split-Path -Parent $output)) | Out-Null
        Copy-Item -LiteralPath $license.FullName -Destination $output
    }
}
[IO.File]::WriteAllLines((Join-Path $Destination 'INDEX.txt'), $index)
Write-Output ("Collected notices for {0} Rust dependencies." -f $packages.Count)
