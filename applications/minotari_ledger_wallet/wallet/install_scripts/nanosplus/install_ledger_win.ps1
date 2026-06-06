[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

$ErrorActionPreference = "Stop"
$Installer = Join-Path $PSScriptRoot "..\install_minotari_ledger.py"
$Python = Get-Command python -ErrorAction SilentlyContinue
if (-not $Python) {
    $Python = Get-Command python3 -ErrorAction SilentlyContinue
}
if (-not $Python) {
    Write-Error "Python 3 is required to install the Minotari Ledger wallet."
}

& $Python.Source $Installer --model nanosplus @Args
exit $LASTEXITCODE
