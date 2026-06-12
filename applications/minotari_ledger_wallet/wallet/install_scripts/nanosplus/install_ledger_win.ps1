[CmdletBinding()]
param(
    [string]$Tag,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Installer = Join-Path (Split-Path -Parent $ScriptDir) "install_minotari_ledger.py"
if (-not (Test-Path $Installer)) {
    $Installer = Join-Path $ScriptDir "install_minotari_ledger.py"
}
if (-not (Test-Path $Installer)) {
    Write-Error "install_minotari_ledger.py was not found next to or above this wrapper."
}

$Python = Get-Command python -ErrorAction SilentlyContinue

if (-not $Python) {
    Write-Error "Python 3 is required to run the Minotari Ledger installer."
}

$InstallerArgs = @()
if ($Tag) {
    $InstallerArgs += @("--tag", $Tag)
}
if ($RemainingArgs) {
    $InstallerArgs += $RemainingArgs
}

& $Python.Source $Installer @InstallerArgs
exit $LASTEXITCODE
