#
# Unified Ledger Installer Launcher for Windows
#
# This script checks prerequisites and launches the Python installer
#

param(
    [switch]$Help,
    [switch]$SkipPrereqCheck
)

if ($Help) {
    Write-Host @"
Minotari Ledger Wallet - Unified Installer for Windows

Usage:
    .\install_minotari_ledger.ps1

This installer will:
  1. Check for Python 3.8+ installation
  2. Install required Python dependencies
  3. Detect your connected Ledger device
  4. Download the correct Minotari firmware
  5. Install the app onto your Ledger

Requirements:
  - Python 3.8 or higher
  - Ledger device connected via USB
  - Device unlocked with Developer Mode enabled

For troubleshooting, see README.md
"@
    exit 0
}

# Helper functions
function Write-ErrorMsg($message) {
    Write-Host "❌ $message" -ForegroundColor Red
}

function Write-Success($message) {
    Write-Host "✅ $message" -ForegroundColor Green
}

function Write-Info($message) {
    Write-Host "ℹ️  $message" -ForegroundColor Cyan
}

function Write-Warning($message) {
    Write-Host "⚠️  $message" -ForegroundColor Yellow
}

# Get script directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$PythonScript = Join-Path $ScriptDir "install_minotari_ledger.py"

# Check if Python script exists
if (-not (Test-Path $PythonScript)) {
    Write-ErrorMsg "Python installer not found: $PythonScript"
    exit 1
}

Write-Info "Minotari Ledger Wallet - Unified Installer for Windows"
Write-Host ""

if (-not $SkipPrereqCheck) {
    Write-Info "Checking prerequisites..."
    
    # Check for Python
    $PythonCmd = $null
    
    # Try python3 first, then python
    $PythonCommands = @("python3", "python", "py")
    
    foreach ($cmd in $PythonCommands) {
        $PythonPath = Get-Command $cmd -ErrorAction SilentlyContinue
        if ($PythonPath) {
            # Verify it's Python 3
            try {
                $VersionOutput = & $cmd --version 2>&1
                if ($VersionOutput -match "Python 3\.(\d+)") {
                    $PythonMinor = [int]$Matches[1]
                    if ($PythonMinor -ge 8) {
                        $PythonCmd = $cmd
                        break
                    }
                }
            } catch {
                # Continue to next command
            }
        }
    }
    
    if (-not $PythonCmd) {
        Write-ErrorMsg "Python 3.8 or higher is required but not found"
        Write-Host ""
        Write-Info "Please install Python from https://python.org"
        Write-Host ""
        Write-Host "During installation, make sure to check:"
        Write-Host "  ☑ 'Add Python to PATH'"
        Write-Host ""
        exit 1
    }
    
    # Get Python version for display
    $VersionOutput = & $PythonCmd --version 2>&1
    Write-Success "Found $VersionOutput"
    
    # Check for pip
    try {
        $PipVersion = & $PythonCmd -m pip --version 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "pip not found"
        }
    } catch {
        Write-ErrorMsg "pip is not installed"
        Write-Info "Please ensure pip is installed with Python"
        exit 1
    }
    
    Write-Success "pip is available"
    
    # Check for USB drivers (informational)
    Write-Info "Checking USB drivers..."
    
    # Check if any Ledger devices are visible in Device Manager
    $LedgerDevices = Get-PnpDevice | Where-Object { 
        $_.FriendlyName -match "Ledger" -or 
        $_.InstanceId -match "VID_2C97"
    } -ErrorAction SilentlyContinue
    
    if ($LedgerDevices) {
        Write-Success "Ledger USB device detected"
    } else {
        Write-Warning "No Ledger device currently detected"
        Write-Info "Please connect your Ledger via USB before continuing"
    }
    
    Write-Host ""
    Write-Success "All prerequisites met"
    Write-Host ""
}

# Run the Python installer
Write-Info "Starting Minotari Ledger Wallet installer..."
Write-Host ""

try {
    & $PythonCmd $PythonScript @args
    $ExitCode = $LASTEXITCODE
} catch {
    Write-ErrorMsg "Failed to run installer: $_"
    $ExitCode = 1
}

exit $ExitCode
