$ErrorActionPreference = "Stop"

Write-Host "🚀 Installing Minotari Ledger Wallet (Nano S Plus)" -ForegroundColor Cyan

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
pip install protobuf setuptools ecdsa ledgerwallet

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

Write-Host "🌐 Fetching latest Minotari Ledger release..."

New-Item -ItemType Directory -Force -Path $DownloadDir | Out-Null
Set-Location $DownloadDir

$Release = Invoke-RestMethod `
    -Uri "https://api.github.com/repos/tari-project/tari/releases/latest" `
    -Headers @{ "User-Agent" = "PowerShell" }

$Asset = $Release.assets |
    Where-Object { $_.name -match "minotari_ledger_wallet-nanosplus.*\.zip" } |
    Select-Object -First 1

if (-not $Asset) {
    Write-Error "Could not find nanosplus release asset."
}

Write-Host "⬇️ Downloading $($Asset.name)"
Invoke-WebRequest $Asset.browser_download_url -OutFile $Asset.name

Write-Host "📦 Extracting archive..."
Expand-Archive -Path $Asset.name -DestinationPath . -Force

# -------------------------
# Install onto Ledger
# -------------------------

$appJson = Get-ChildItem -Recurse -Filter "app_nanosplus.json" | Select-Object -First 1

if (-not $appJson) {
    Write-Error "app_nanosplus.json not found after extraction."
}

Write-Host ""
Write-Host "🔐 Installing app onto Ledger Nano S Plus..." -ForegroundColor Yellow
Write-Host "👉 Ensure:"
Write-Host "   • Ledger connected via USB"
Write-Host "   • Device unlocked"
Write-Host "   • Developer Mode enabled"
Write-Host ""

ledgerctl install $appJson.FullName

Write-Host ""
Write-Host "✅ Minotari Ledger Wallet installed successfully!" -ForegroundColor Green
