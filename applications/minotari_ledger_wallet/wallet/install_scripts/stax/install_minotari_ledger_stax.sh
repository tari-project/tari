# To run
# chmod +x install_minotari_ledger_stax.sh
# ./install_minotari_ledger_stax.sh


#!/usr/bin/env bash

set -euo pipefail

# -------------------------
# CLI options
# -------------------------

RELEASE_TAG=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [-t TAG]

Options:
  -t, --tag TAG    Install a specific release tag (e.g. v5.2.0-pre.7), including
                   pre-releases. Defaults to the latest published release.
  -h, --help       Show this help and exit.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -t|--tag)
      [[ $# -ge 2 ]] || { echo "❌ $1 requires a value"; usage; exit 1; }
      RELEASE_TAG="$2"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "❌ Unknown argument: $1"; usage; exit 1 ;;
  esac
done

echo "🚀 Installing Minotari Ledger Wallet (Stax)"

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

if [[ -n "$RELEASE_TAG" ]]; then
  echo "🌐 Fetching Minotari Ledger release info for tag '$RELEASE_TAG'..."
  RELEASE_API="https://api.github.com/repos/tari-project/tari/releases/tags/${RELEASE_TAG}"
else
  echo "🌐 Fetching latest Minotari Ledger release info..."
  RELEASE_API="https://api.github.com/repos/tari-project/tari/releases/latest"
fi

ASSET_URL=$(curl -fsSL "$RELEASE_API" \
  | jq -r '
      .assets[]
      | select(.name | test("minotari_ledger_wallet-stax.*\\.zip$"))
      | .browser_download_url
    ')

if [[ -z "$ASSET_URL" || "$ASSET_URL" == "null" ]]; then
  echo "❌ Could not find stax release asset."
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

APP_JSON=$(find . -name "app_stax.json" | head -n 1)

if [[ -z "$APP_JSON" ]]; then
  echo "❌ app_stax.json not found after unzip."
  exit 1
fi

echo
echo "🔐 Installing app onto Ledger Stax..."
echo "👉 Ensure:"
echo "   • Ledger connected via USB"
echo "   • Device unlocked"
echo "   • Developer Mode enabled"
echo

ledgerctl install "$APP_JSON"

echo
echo "✅ Minotari Ledger Wallet installed successfully!"
