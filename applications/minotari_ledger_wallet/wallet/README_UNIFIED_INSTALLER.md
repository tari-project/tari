# Unified Minotari Ledger Wallet Installer

This directory contains a single, cross-platform installer for the Minotari Ledger Wallet
that works on macOS, Windows, and Linux.

## Features

- Auto-detects your connected Ledger model (Flex, Nano S, Nano S Plus, Nano X, Stax)
- Cross-platform: works on Windows, macOS, and Linux
- Single command: download, extract, and install in one step
- Error handling: clear errors if device is not connected or unsupported
- Manual fallback: select model if auto-detection fails

## Quick Start

### Windows

1. Prerequisites: Install Python 3: https://www.python.org/downloads/
2. Run the installer:

```powershell
.\install_minotari_ledger.ps1
```

### macOS/Linux

1. Prerequisites: Python 3 (usually pre-installed)
   - macOS: brew install python3
   - Linux: apt-get install python3 (Debian/Ubuntu) or equivalent
2. Make the script executable:

```bash
chmod +x install_minotari_ledger.sh
```

3. Run the installer:

```bash
./install_minotari_ledger.sh
```

## What the Installer Does

1. Checks system requirements (Python, pip)
2. Installs required Python packages (ledgerctl, protobuf, etc.)
3. Auto-detects your Ledger device model
4. Downloads the latest Minotari Ledger release for your model
5. Extracts the app manifest
6. Installs the app onto your Ledger device

## Prerequisites

- Python 3.7+ with pip
- USB connection to your Ledger device

## Device Setup

Before running the installer, ensure your Ledger device is:
- Connected via USB
- Unlocked (enter PIN)
- Developer Mode enabled (if applicable for your model)

## Supported Models

- Ledger Flex
- Ledger Nano S
- Ledger Nano S Plus
- Ledger Nano X
- Ledger Stax

## Troubleshooting

### Python not found
- Windows: Install Python 3 from https://www.python.org/downloads/ (check Add to PATH)
- macOS: brew install python3
- Linux: apt-get install python3 (Debian/Ubuntu)

### Device not detected
- Ensure your Ledger is connected via USB
- Unlock the device (enter PIN)
- Try disconnecting and reconnecting the USB cable
- Try a different USB port

### ledgerctl not found after installation
- The installer should auto-install ledgerctl via pip
- If issues persist, try: pip install ledgerctl

### Installation hangs or times out
- Check device is still connected
- Make sure it is not showing a confirmation dialog
- Restart the installer if needed

## Advanced Usage

### Using the Python Script Directly

```bash
# macOS/Linux
python3 install_minotari_ledger.py

# Windows
python install_minotari_ledger.py
```

### Manual Model Selection

If auto-detection fails, the installer will prompt you to select your model.

## Legacy Installation Scripts

The old per-model installation scripts are available in separate directories:
- flex/ - Ledger Flex
- nanosplus/ - Ledger Nano S Plus
- nanox/ - Ledger Nano X
- stax/ - Ledger Stax

Each has:
- install_minotari_ledger_[model].sh (macOS/Linux)
- install_ledger_win.ps1 (Windows)

These are maintained for compatibility but the unified installer is recommended.

## Support

If you encounter issues:
1. Check the troubleshooting section above
2. Check your device firmware is up to date
3. Ask in the Tari Discord: https://discord.gg/tari
