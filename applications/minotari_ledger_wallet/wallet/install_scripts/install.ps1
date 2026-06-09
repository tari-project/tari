# Tari Ledger Installer Wrapper (PowerShell)
# This script ensures Astral's `uv` is installed, then runs the Python script
# in a completely isolated environment without needing system Python/pip configuration.

$ErrorActionPreference = "Stop"

if (-not (Get-Command "uv" -ErrorAction SilentlyContinue)) {
    Write-Host "==> 'uv' not found. Installing Astral's uv for isolated execution..." -ForegroundColor Cyan
    powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    
    # Reload PATH locally for this process
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$PythonScript = Join-Path $ScriptDir "install_minotari_ledger.py"

# Run the unified python installer seamlessly
uv run $PythonScript
