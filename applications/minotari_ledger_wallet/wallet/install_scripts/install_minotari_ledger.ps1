#Requires -Version 5.1
<#
.SYNOPSIS
    install_minotari_ledger.ps1 — Windows launcher for the Minotari Ledger installer.

.DESCRIPTION
    Checks Python, creates a virtual environment, and runs the unified
    install_minotari_ledger.py which auto-detects your Ledger model.

.EXAMPLE
    .\install_minotari_ledger.ps1
#>

$ErrorActionPreference = "Stop"

Write-Host "\U0001f680 Minotari Ledger Wallet — Windows Launcher" -ForegroundColor Cyan
Write-Host ""

# Check Python
$Python = $null
foreach ($candidate in @("python", "python3")) {
    try {
        $ver = & $candidate --version 2>&1
        if ($ver -match "Python 3") {
            $Python = $candidate
            Write-Host "   Found $ver" -ForegroundColor Green
            break
        }
    } catch {}
}

if (-not $Python) {
    Write-Error "Python 3 is required but not found on PATH. Install from https://python.org"
    exit 1
}

# Create / reuse venv
$VenvDir = "$env:USERPROFILE\.minotari-ledger-installer"

if (-not (Test-Path $VenvDir)) {
    Write-Host "\U0001f40d Creating Python virtual environment at $VenvDir..." -ForegroundColor Yellow
    & $Python -m venv $VenvDir
}

$ActivateScript = "$VenvDir\Scripts\Activate.ps1"
if (-not (Test-Path $ActivateScript)) {
    Write-Error "Could not find venv activation script at $ActivateScript"
    exit 1
}

. $ActivateScript

# Run installer
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$InstallerScript = Join-Path $ScriptDir "install_minotari_ledger.py"

if (-not (Test-Path $InstallerScript)) {
    Write-Error "Installer script not found: $InstallerScript"
    exit 1
}

& python $InstallerScript @args
