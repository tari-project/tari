[CmdletBinding()]
param(
    [switch]$Help,
    [string]$Tag,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Installer = Join-Path $ScriptDir "install_minotari_ledger.py"

if (-not (Test-Path $Installer)) {
    Write-Error "install_minotari_ledger.py was not found next to this launcher."
}

# The installer declares its Ledger tooling inline (PEP 723). `uv run` provisions
# an isolated Python plus those dependencies on demand, so no system Python or pip
# setup is required.
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "==> 'uv' was not found; installing it from https://astral.sh/uv for isolated execution..." -ForegroundColor Cyan
    powershell -ExecutionPolicy Bypass -Command "irm https://astral.sh/uv/install.ps1 | iex"

    # Make uv available in this session from its default install locations.
    $env:Path = "$HOME\.local\bin;$HOME\.cargo\bin;$env:Path"
}

if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Error "uv is still unavailable after installation. Open a new terminal and rerun, or install uv from https://docs.astral.sh/uv/."
}

$InstallerArgs = @()
if ($Help) {
    $InstallerArgs += "--help"
}
if ($Tag) {
    $InstallerArgs += @("--tag", $Tag)
}
if ($RemainingArgs) {
    $InstallerArgs += $RemainingArgs
}

& uv run $Installer @InstallerArgs
exit $LASTEXITCODE
