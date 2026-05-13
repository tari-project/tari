# Minotari Ledger Wallet - Unified Installer

A single cross-platform installer for the Minotari Ledger Wallet app that auto-detects your Ledger device model and installs the correct firmware.

## Features

- ✅ **Single installer** for macOS, Windows, and Linux
- ✅ **Auto-detection** of Ledger model (Nano S Plus, Nano X, Stax, Flex)
- ✅ **Automatic download** of correct firmware from GitHub releases
- ✅ **Clear error handling** with helpful messages
- ✅ **No manual configuration** required

## Supported Devices

| Device | Support Status |
|--------|---------------|
| Ledger Nano S Plus | ✅ Supported |
| Ledger Nano X | ✅ Supported |
| Ledger Stax | ✅ Supported |
| Ledger Flex | ✅ Supported |
| Ledger Nano S (legacy) | ❌ Not Supported |

## Quick Start

### macOS / Linux

```bash
cd applications/minotari_ledger_wallet/wallet/install_scripts
chmod +x install_minotari_ledger.sh
./install_minotari_ledger.sh
```

### Windows (PowerShell)

```powershell
cd applications\minotari_ledger_wallet\wallet\install_scripts
.\install_minotari_ledger.ps1
```

### Direct Python (All Platforms)

```bash
python3 install_minotari_ledger.py
```

## Prerequisites

### All Platforms

- Python 3.8 or higher
- Ledger device connected via USB
- Device unlocked
- Developer Mode enabled (Settings > Developer)

### macOS

- Homebrew recommended for dependencies

### Linux

- libusb-1.0-0-dev (Ubuntu/Debian) or libusb-devel (Fedora)

### Windows

- USB drivers (usually installed automatically)

## How It Works

1. **Device Detection**: The installer first tries to detect your Ledger via USB HID, falling back to `ledgerctl info` if needed
2. **Model Identification**: Maps the USB product ID to the correct Ledger model
3. **Firmware Download**: Fetches the latest release from GitHub and downloads the correct asset for your model
4. **Installation**: Uses `ledgerctl` to install the app onto your device

## Troubleshooting

### "No Ledger device detected"

- Ensure your Ledger is connected via USB
- Unlock the device
- Enable Developer Mode: Settings > Developer > Yes
- Try a different USB cable or port

### "No firmware found for Ledger [model]"

- Check that a release exists with Ledger wallet assets
- The asset naming follows: `minotari_ledger_wallet-{model}-v{version}.zip`

### Python/pip not found

**macOS:**
```bash
brew install python3
```

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install python3 python3-pip
```

**Fedora:**
```bash
sudo dnf install python3 python3-pip
```

### HID detection fails on Linux

Install libusb:
```bash
# Ubuntu/Debian
sudo apt-get install libusb-1.0-0-dev

# Fedora
sudo dnf install libusb-devel

# Arch
sudo pacman -S libusb
```

## Testing

Run the test suite:

```bash
python3 test_installer.py
```

Or with pytest:

```bash
python3 -m pytest test_installer.py -v
```

## Architecture

```
install_scripts/
├── install_minotari_ledger.py      # Core Python installer
├── install_minotari_ledger.sh      # macOS/Linux launcher
├── install_minotari_ledger.ps1     # Windows launcher
├── test_installer.py               # Unit tests
└── README.md                       # This file
```

## Legacy Scripts

The old per-model installation scripts are still available in subdirectories:
- `flex/` - Ledger Flex
- `nanosplus/` - Ledger Nano S Plus
- `nanox/` - Ledger Nano X
- `stax/` - Ledger Stax

These are maintained for compatibility but the unified installer is recommended.

## Development

### Adding Support for New Ledger Models

1. Add the product ID to `LEDGER_PRODUCT_IDS` in `install_minotari_ledger.py`
2. Add the display name to `MODEL_NAMES`
3. Update tests in `test_installer.py`
4. Update this README

### Testing Changes

```bash
# Run all tests
python3 test_installer.py

# Test specific component
python3 -m pytest test_installer.py::TestLedgerProductIds -v
```

## Security Notes

- The installer downloads firmware directly from the official Tari GitHub releases
- Always verify the download URL is `github.com/tari-project/tari`
- The installer requires no sudo/admin privileges (except for initial dependency installation)

## License

Same as the main Tari project.

## Contributing

Please follow the existing code style and add tests for new features.

## Fixes Over Previous Attempts

This implementation addresses issues found in previous PRs:

### vs PR #7805
- **Fixed**: Uses correct model slug `nanosplus` instead of `nanos+` (which caused 404 errors)
- **Fixed**: Proper substring matching in ledgerctl detection (avoids confusing Nano S Plus with Nano S)
- **Fixed**: Uses `sys.executable -m ledgerwallet` instead of bare `ledgerctl` for venv compatibility
- **Fixed**: Proper temp directory cleanup with context managers
- **Added**: Comprehensive test coverage

### vs PR #7803
- **Fixed**: All review comments addressed
- **Added**: Better error handling and user messages
- **Added**: Streaming download with progress indicator
- **Added**: Full test suite
