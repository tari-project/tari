# Too run
# chmod +x install_minotari_ledger_nanox.sh
# ./install_minotari_ledger_nanox.sh


#!/usr/bin/env bash

set -euo pipefail

echo "🚀 Installing Minotari Ledger Wallet (Nano S Plus)"

# -------------------------
# Prerequisites
# -------------------------

if ! command -v brew >/dev/null 2>&1; then
  echo "❌ Homebrew is not installed. Install it first from https://brew.sh"
  exit 1
fi

echo "🔧 Installing system dependencies..."
brew install virtualenv wget jq

# -------------------------
# Project setup
# -------------------------

PROJECT_DIR="$HOME/src/tari"
VENV_DIR="$PROJECT_DIR/tari-ledger-live"
DOWNLOAD_DIR="$VENV_DIR/tari-downloads"

echo "📁 Setting up project directory at $PROJECT_DIR"
mkdir -p "$PROJECT_DIR"
cd "$PROJECT_DIR"

if [[ ! -d "$VENV_DIR" ]]; then
  echo "🐍 Creating Python virtual environment..."
  python3 -m venv "$VENV_DIR"
fi

cd "$VENV_DIR"
# shellcheck disable=SC1091
source bin/activate

echo "📦 Installing Python dependencies..."
pip3 install --upgrade pip
pip3 install protobuf setuptools ecdsa ledgerwallet

# -------------------------
# Auto-install ledgerctl
# -------------------------

if ! command -v ledgerctl >/dev/null 2>&1; then
  echo "🔐 ledgerctl not found — installing..."
  pip3 install ledgerctl
else
  echo "✅ ledgerctl already installed"
fi

mkdir -p "$DOWNLOAD_DIR"
cd "$DOWNLOAD_DIR"

# -------------------------
# Download latest release
# -------------------------

echo "🌐 Fetching latest Minotari Ledger release info..."


ASSET_URL=$(curl -fsSL https://api.github.com/repos/tari-project/tari/releases/latest \
  | jq -r '
      .assets[]
      | select(.name | test("minotari_ledger_wallet-nanox.*\\.zip$"))
      | .browser_download_url
    ')

if [[ -z "$ASSET_URL" || "$ASSET_URL" == "null" ]]; then
  echo "❌ Could not find nanox release asset."
  exit 1
fi

echo "⬇️  Downloading:"
echo "   $ASSET_URL"

wget -q --show-progress "$ASSET_URL"

ZIP_FILE=$(basename "$ASSET_URL")

echo "📦 Unzipping $ZIP_FILE"
unzip -o "$ZIP_FILE"

# -------------------------
# Install onto Ledger
# -------------------------

APP_JSON=$(find . -name "app_nanox.json" | head -n 1)

if [[ -z "$APP_JSON" ]]; then
  echo "❌ app_nanox.json not found after unzip."
  exit 1
fi

echo
echo "🔐 Installing app onto Ledger Nano X..."
echo "👉 Ensure:"
echo "   • Ledger connected via USB"
echo "   • Device unlocked"
echo "   • Developer Mode enabled"
echo

ledgerctl install "$APP_JSON"

echo
echo "✅ Minotari Ledger Wallet installed successfully!"
