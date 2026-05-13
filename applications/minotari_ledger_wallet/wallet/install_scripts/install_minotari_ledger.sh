#!/usr/bin/env bash
#
# Unified Ledger Installer Launcher for macOS and Linux
#
# This script checks prerequisites and launches the Python installer
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_error() {
    echo -e "${RED}❌ $1${NC}" >&2
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_SCRIPT="$SCRIPT_DIR/install_minotari_ledger.py"

# Check if Python script exists
if [[ ! -f "$PYTHON_SCRIPT" ]]; then
    print_error "Python installer not found: $PYTHON_SCRIPT"
    exit 1
fi

# Detect OS
OS="$(uname -s)"
case "${OS}" in
    Linux*)     PLATFORM=Linux;;
    Darwin*)    PLATFORM=Mac;;
    CYGWIN*)    PLATFORM=Cygwin;;
    MINGW*)     PLATFORM=MinGw;;
    *)          PLATFORM="UNKNOWN:${OS}"
esac

print_info "Detected platform: $PLATFORM"

# Check for Python 3
if command -v python3 >/dev/null 2>&1; then
    PYTHON_CMD="python3"
elif command -v python >/dev/null 2>&1; then
    # Check if it's Python 3
    PY_VERSION=$(python --version 2>&1 | awk '{print $2}' | cut -d. -f1)
    if [[ "$PY_VERSION" == "3" ]]; then
        PYTHON_CMD="python"
    else
        print_error "Python 3 is required but Python 2 was found"
        exit 1
    fi
else
    print_error "Python 3 is not installed"
    print_info "Please install Python 3.8 or higher:"
    
    case "${PLATFORM}" in
        Mac)
            echo "  brew install python3"
            ;;
        Linux)
            if command -v apt-get >/dev/null 2>&1; then
                echo "  sudo apt-get update"
                echo "  sudo apt-get install python3 python3-pip"
            elif command -v dnf >/dev/null 2>&1; then
                echo "  sudo dnf install python3 python3-pip"
            elif command -v pacman >/dev/null 2>&1; then
                echo "  sudo pacman -S python python-pip"
            else
                echo "  Please use your distribution's package manager to install Python 3"
            fi
            ;;
        *)
            echo "  Please install Python 3.8+ from https://python.org"
            ;;
    esac
    exit 1
fi

# Check Python version
PY_VERSION_FULL=$($PYTHON_CMD --version 2>&1 | awk '{print $2}')
PY_MAJOR=$(echo "$PY_VERSION_FULL" | cut -d. -f1)
PY_MINOR=$(echo "$PY_VERSION_FULL" | cut -d. -f2)

if [[ "$PY_MAJOR" -lt 3 ]] || [[ "$PY_MAJOR" -eq 3 && "$PY_MINOR" -lt 8 ]]; then
    print_error "Python 3.8+ required, found $PY_VERSION_FULL"
    exit 1
fi

print_success "Python $PY_VERSION_FULL found"

# Check for required system libraries
print_info "Checking system dependencies..."

case "${PLATFORM}" in
    Mac)
        # macOS usually has everything needed
        if ! command -v brew >/dev/null 2>&1; then
            print_warning "Homebrew not found. Some dependencies may need manual installation."
        fi
        ;;
    Linux)
        # Check for libusb/hidapi
        if command -v ldconfig >/dev/null 2>&1; then
            if ! ldconfig -p | grep -q libusb; then
                print_warning "libusb not detected. You may need to install it:"
                if command -v apt-get >/dev/null 2>&1; then
                    echo "  sudo apt-get install libusb-1.0-0-dev"
                elif command -v dnf >/dev/null 2>&1; then
                    echo "  sudo dnf install libusb-devel"
                elif command -v pacman >/dev/null 2>&1; then
                    echo "  sudo pacman -S libusb"
                fi
            fi
        fi
        ;;
esac

# Check for pip
if ! $PYTHON_CMD -m pip --version >/dev/null 2>&1; then
    print_error "pip is not installed"
    print_info "Please install pip for Python 3"
    exit 1
fi

print_success "All prerequisites met"
echo

# Run the Python installer
print_info "Starting Minotari Ledger Wallet installer..."
echo

exec "$PYTHON_CMD" "$PYTHON_SCRIPT" "$@"
