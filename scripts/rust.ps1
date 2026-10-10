param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $CargoArguments
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$localCargo = Join-Path $projectRoot '.tools/cargo/bin/cargo.exe'
if (Test-Path -LiteralPath $localCargo) {
    $env:CARGO_HOME = Join-Path $projectRoot '.tools/cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.tools/rustup'
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    & $localCargo @CargoArguments
} else {
    & cargo @CargoArguments
}
exit $LASTEXITCODE
