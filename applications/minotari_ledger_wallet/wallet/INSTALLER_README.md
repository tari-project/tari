# Minotari Ledger Wallet - Unified Installer

Replaces per-OS/per-model scripts with a single cross-platform installer.

## Usage

```bash
pip install requests ledgerwallet ledgerblue hidapi
python3 minotari_ledger_install.py          # auto-detect, latest release
python3 minotari_ledger_install.py --tag v5.2.0
python3 minotari_ledger_install.py --model nanosplus
```

Models: `nanos` | `nanosplus` | `nanox` | `flex` | `stax`

## Prerequisites

Python 3.10+, Ledger connected via USB, unlocked, Developer Mode on.
Works on macOS, Linux, and Windows.
