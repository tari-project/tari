# Minotari Ledger Wallet — Unified Installer

A single cross-platform installer that automatically detects your connected Ledger hardware wallet model and installs the correct Minotari app — no manual model selection needed.

## Supported Ledger Models

> Minotari does not ship an app for the original Nano S, only the Nano S Plus. The original Nano S is intentionally **not** detected by this installer.

| Model | Slug | Detection Method |
|---|---|---|
| Nano S Plus | `nanosplus` | USB HID (PID 0x0004 / 0x4000) |
| Nano X | `nanox` | USB HID (PID 0x0005 / 0x5000) |
| Stax | `stax` | USB HID (PID 0x0006 / 0x6000) |
| Flex | `flex` | USB HID (PID 0x0007 / 0x7000) |

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

## How It Works

1. **Dependency setup** — Creates an isolated Python virtual environment and installs `ledgerctl` and the `hid` package
2. **Device detection** — Enumerates USB HID devices filtered by Ledger's vendor ID (`0x2C97`), maps the product ID to a model slug. Falls back to `ledgerctl info` output parsing if the `hid` library is unavailable
3. **Download** — Fetches the latest release from GitHub and selects the correct asset for the detected model
4. **Install** — Extracts the release archive and calls `ledgerctl install` with the matching `app_<model>.json` manifest
5. **Cleanup** — Removes temporary download and extraction files

## Troubleshooting

- **No device detected**: Ensure the Ledger is connected via USB, unlocked, Developer Mode is enabled, and no other app is running on the device
- **Permission denied (Linux)**: Install udev rules for Ledger devices. See https://support.ledger.com/hc/en-us/articles/360018300334
- **libusb errors (Linux)**: Install with `sudo apt-get install libusb-1.0-0`
- **Download fails**: Check your internet connection. If behind a proxy, set `HTTPS_PROXY` environment variable

## Security

- Downloads are from the official Tari GitHub releases (`tari-project/tari`)
- All dependencies are installed in an isolated virtual environment
- No keys or secrets are accessed
- Installation requires physical confirmation on the Ledger device

## License

Part of the Tari Project. See [LICENSE](../../LICENSE) for details.
