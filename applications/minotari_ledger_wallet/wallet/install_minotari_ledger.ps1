# Unified Minotari Ledger Wallet Installer for Windows
# This script runs the cross-platform Python installer

$ErrorActionPreference = "Stop"

Write-Host "Minotari Ledger Wallet Installer for Windows" -ForegroundColor Cyan

if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Host "Python is not installed or not on PATH." -ForegroundColor Red
    Write-Host "Install Python 3 from https://www.python.org/downloads/" -ForegroundColor Yellow
    exit 1
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$PythonInstallerPath = Join-Path $ScriptDir "install_minotari_ledger.py"

if (-not (Test-Path $PythonInstallerPath)) {
    Write-Host "install_minotari_ledger.py not found at $PythonInstallerPath" -ForegroundColor Red
    exit 1
}

Write-Host "Using Python installer: $PythonInstallerPath" -ForegroundColor Cyan

try {
    python $PythonInstallerPath
} catch {
    Write-Host "Installation failed: $_" -ForegroundColor Red
    exit 1
}
