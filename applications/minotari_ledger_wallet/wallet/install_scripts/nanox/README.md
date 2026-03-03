# Minotari Ledger Nano X Installer (macOS)

This script installs the **Minotari Ledger Wallet app** (`minotari_ledger_wallet-nanox`) onto a **Ledger Nano S Plus** on macOS.

It is fully automated and handles:
- System dependencies
- Python virtual environment setup
- `ledgerctl` installation
- Downloading the **latest** Minotari Ledger release
- Installing the app onto the Ledger device

---

## Supported Platforms

- **macOS** (Intel & Apple Silicon)
- **Ledger Nano S Plus**

---

## What the Script Does

1. Installs required tools via **Homebrew**
2. Creates a Python **virtual environment**
3. Installs required Python dependencies
4. Automatically installs `ledgerctl` (if missing)
5. Downloads the **latest** `minotari_ledger_wallet-nanox` release from GitHub
6. Unzips the release
7. Uploads the app to the Ledger device using `ledgerctl`

All tooling is isolated inside the virtual environment to avoid polluting system Python.

---

## Prerequisites

### Homebrew

```bash
brew --version
```

If not installed, see https://brew.sh

### Ledger Device

Ensure your **Ledger Nano S Plus** is:
- Connected via USB
- Unlocked
- Developer Mode enabled
- Not running another app

---

## Installation

```bash
chmod +x install_minotari_ledger_nanox.sh
./install_minotari_ledger_nanox.sh
```

---

## Directory Layout

```text
~/src/tari/
└── tari-ledger-live/
    ├── bin/
    ├── lib/
    └── tari-downloads/
```

---

## Re-running the Script

The script is safe to re-run and will always fetch the latest release.

---

## Troubleshooting

- Ensure the Ledger is unlocked and Developer Mode is enabled
- Close Ledger Live before installing
- Use a data-capable USB cable

---

## Security

- Downloads are from the official Tari GitHub
- No keys or secrets are accessed
- Installation requires physical confirmation on the Ledger

---

## License

Provided as-is. Minotari Ledger app is licensed by the Tari Project.
