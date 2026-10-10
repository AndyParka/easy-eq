param([switch] $Release)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    $cargoArguments = @('build', '--manifest-path', 'crates/easy-eq-desktop/Cargo.toml', '--locked')
    if ($Release) { $cargoArguments += '--release' }
    & (Join-Path $PSScriptRoot 'rust.ps1') @cargoArguments
    exit $LASTEXITCODE
} finally { Pop-Location }
