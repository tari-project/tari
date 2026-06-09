# Minotari Ledger Installer

This directory contains the unified installation script for the Minotari Ledger application.

It automatically detects your connected Ledger device (Nano S Plus, Nano X, Stax, or Flex), downloads the correct firmware from the latest Tari release, and securely flashes it to your device.

## Prerequisites
- A Ledger device connected via USB.
- The device must be unlocked (PIN entered) and on the dashboard.
- **No Python or dependencies required.** The script utilizes [Astral's uv](https://docs.astral.sh/uv/) to automatically bootstrap an isolated environment.

## How to Install

### macOS & Linux
1. Open your terminal.
2. Run the shell script:
```bash
sh install.sh
```

### Windows
1. Open PowerShell.
2. Run the script:
```powershell
.\install.ps1
```

If you encounter an execution policy error, run this first:
```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
```

## How it works
The `install_minotari_ledger.py` script leverages PEP 723 inline metadata to declare its dependencies (`ledgerwallet` and `ledgerctl`). The `uv run` command instantly resolves these dependencies without polluting your system environment, detects your specific Ledger model using `hidapi`, and downloads the matching `app_{slug}.json` directly from the Tari GitHub releases page.
