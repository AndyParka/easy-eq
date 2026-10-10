param([switch] $Release)
$ErrorActionPreference = 'Stop'
$cargoArguments = @('build', '--manifest-path', (Join-Path $PSScriptRoot '../crates/easy-eq-vst/Cargo.toml'))
if ($Release) { $cargoArguments += '--release' }
& (Join-Path $PSScriptRoot 'rust.ps1') @cargoArguments
exit $LASTEXITCODE
