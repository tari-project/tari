[CmdletBinding()]
param(
    # Install a specific release tag (e.g. v5.2.0-pre.7), including pre-releases.
    # If omitted, the latest published release is used.
    [string]$Tag
)

$ErrorActionPreference = "Stop"

Write-Host "🚀 Installing Minotari Ledger Wallet (Stax)" -ForegroundColor Cyan

# -------------------------
# Prerequisites
# -------------------------

if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Error "Python is not installed or not on PATH. Install Python 3 first."
}

if (-not (Get-Command pip -ErrorAction SilentlyContinue)) {
    Write-Error "pip not found. Ensure Python was installed with pip enabled."
}

# -------------------------
# Project setup
# -------------------------

$ProjectDir = "$env:USERPROFILE\src\tari"
$VenvDir    = "$ProjectDir\tari-ledger-live"
$DownloadDir = "$VenvDir\tari-downloads"

Write-Host "📁 Setting up project directory at $ProjectDir"
New-Item -ItemType Directory -Force -Path $ProjectDir | Out-Null
Set-Location $ProjectDir

if (-not (Test-Path $VenvDir)) {
    Write-Host "🐍 Creating Python virtual environment..."
    python -m venv $VenvDir
}

# Activate venv
& "$VenvDir\Scripts\Activate.ps1"

Write-Host "📦 Installing Python dependencies..."
pip install --upgrade pip
pip install protobuf setuptools ecdsa ledgerwallet ledgerblue

# -------------------------
# Auto-install ledgerctl
# -------------------------

if (-not (Get-Command ledgerctl -ErrorAction SilentlyContinue)) {
    Write-Host "🔐 ledgerctl not found — installing..."
    pip install ledgerctl
} else {
    Write-Host "✅ ledgerctl already installed"
}

# -------------------------
# Download latest release
# -------------------------

if ($Tag) {
    Write-Host "🌐 Fetching Minotari Ledger release info for tag '$Tag'..."
    $ReleaseUri = "https://api.github.com/repos/tari-project/tari/releases/tags/$Tag"
} else {
    Write-Host "🌐 Fetching latest Minotari Ledger release..."
    $ReleaseUri = "https://api.github.com/repos/tari-project/tari/releases/latest"
}

New-Item -ItemType Directory -Force -Path $DownloadDir | Out-Null
Set-Location $DownloadDir

$Release = Invoke-RestMethod `
    -Uri $ReleaseUri `
    -Headers @{ "User-Agent" = "PowerShell" }

$Asset = $Release.assets |
    Where-Object { $_.name -match "minotari_ledger_wallet-stax.*\.zip" } |
    Select-Object -First 1

if (-not $Asset) {
    Write-Error "Could not find stax release asset."
}

Write-Host "⬇️ Downloading $($Asset.name)"
Invoke-WebRequest $Asset.browser_download_url -OutFile $Asset.name

Write-Host "📦 Extracting archive..."
Expand-Archive -Path $Asset.name -DestinationPath . -Force

# -------------------------
# Install onto Ledger
# -------------------------

# cargo-ledger no longer emits an app_<device>.json manifest; the build now
# produces a self-contained .apdu install script instead.
$appApdu = Get-ChildItem -Recurse -Filter "minotari_ledger_wallet.apdu" | Select-Object -First 1

if (-not $appApdu) {
    Write-Error "minotari_ledger_wallet.apdu not found after extraction."
}

Write-Host ""
Write-Host "🔐 Installing app onto Ledger Stax..." -ForegroundColor Yellow
Write-Host "👉 Ensure:"
Write-Host "   • Ledger connected via USB"
Write-Host "   • Device unlocked"
Write-Host "   • Developer Mode enabled"
Write-Host ""

# Remove any previous install (best effort) so the fresh load does not clash.
try { ledgerctl delete "MinoTari Wallet" 2>$null } catch {}

# Replay the .apdu install script over a secure channel (Stax target id).
python -m ledgerblue.runScript --targetId 0x33200004 --fileName $appApdu.FullName --apdu --scp

Write-Host ""
Write-Host "✅ Minotari Ledger Wallet installed successfully!" -ForegroundColor Green
