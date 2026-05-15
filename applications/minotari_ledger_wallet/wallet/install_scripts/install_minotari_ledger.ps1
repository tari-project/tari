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

Write-Host "🚀 Minotari Ledger Wallet — Windows Launcher" -ForegroundColor Cyan
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

# Determine script directory (same directory as this PS1 file)
$ScriptDir = $PSScriptRoot
if (-not $ScriptDir) {
    $ScriptDir = (Get-Location).Path
}

$InstallerPy = Join-Path $ScriptDir "install_minotari_ledger.py"

if (-not (Test-Path $InstallerPy)) {
    Write-Error "install_minotari_ledger.py not found in $ScriptDir"
    exit 1
}

# Run the Python installer
& "$InstallerPy" @args
exit $LASTEXITCODE
