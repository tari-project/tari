# Minotari Ledger Wallet — Unified Installer

A single cross-platform installer that automatically detects your connected Ledger hardware wallet model and installs the correct Minotari app — no manual model selection needed.

## Supported Ledger Models

| Model | Detected via |
|---|---|
| Nano S | USB HID (PID 0x0001 / 0x1000) |
| Nano S Plus | USB HID (PID 0x0004 / 0x4000) |
| Nano X | USB HID (PID 0x0005 / 0x5000) |
| Stax | USB HID (PID 0x0006 / 0x6000) |
| Flex | USB HID (PID 0x0007 / 0x7000) |

## Requirements

- Python 3.8 or later
- Ledger device connected via USB, unlocked, with **Developer Mode** enabled

## Usage

### macOS / Linux

```bash
chmod +x install_minotari_ledger.sh
./install_minotari_ledger.sh
```

### Windows (PowerShell)

```powershell
.\install_minotari_ledger.ps1
```

### Direct Python (any platform)

```bash
python3 install_minotari_ledger.py
```

## What it does

1. Installs Python dependencies (`ledgerctl`, `hid`, etc.) into an isolated virtual environment
2. Detects the connected Ledger model via USB HID (with `ledgerctl info` as fallback)
3. Fetches the latest Minotari release from GitHub
4. Downloads and extracts the correct firmware zip for your model
5. Installs the app onto the Ledger via `ledgerctl install`

## Troubleshooting

**No device detected:**
- Make sure the Ledger is connected via USB (not Bluetooth)
- Enter your PIN to unlock the device
- Enable Developer Mode: Settings → Security → Developer Mode

**`ledgerctl` errors on Linux (permission denied on USB):**
```bash
wget -q -O - https://raw.githubusercontent.com/LedgerHQ/udev-rules/master/add_udev_rules.sh | sudo bash
```

**macOS: run without `sudo`** — macOS grants HID access to the current user session automatically.
