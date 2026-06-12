# To run
# chmod +x install_minotari_ledger_flex.sh
# ./install_minotari_ledger_flex.sh


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

echo "🚀 Installing Minotari Ledger Wallet (Flex)"

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
pip3 install protobuf setuptools ecdsa ledgerwallet ledgerblue

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
      | select(.name | test("minotari_ledger_wallet-flex.*\\.zip$"))
      | .browser_download_url
    ')

if [[ -z "$ASSET_URL" || "$ASSET_URL" == "null" ]]; then
  echo "❌ Could not find flex release asset."
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

# cargo-ledger no longer emits an app_<device>.json manifest; the build now
# produces a self-contained .apdu install script instead.
APP_APDU=$(find . -name "minotari_ledger_wallet.apdu" | head -n 1)

if [[ -z "$APP_APDU" ]]; then
  echo "❌ minotari_ledger_wallet.apdu not found after unzip."
  exit 1
fi

echo
echo "🔐 Installing app onto Ledger Flex..."
echo "👉 Ensure:"
echo "   • Ledger connected via USB"
echo "   • Device unlocked"
echo "   • Developer Mode enabled"
echo

# Remove any previous install (best effort) so the fresh load does not clash.
ledgerctl delete "MinoTari Wallet" || true

# Replay the .apdu install script over a secure channel (Flex target id).
python3 -m ledgerblue.runScript \
  --targetId 0x33300004 \
  --fileName "$APP_APDU" \
  --apdu --scp

echo
echo "✅ Minotari Ledger Wallet installed successfully!"
