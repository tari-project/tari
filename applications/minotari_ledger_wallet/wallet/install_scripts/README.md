# Minotari Ledger Wallet Installer

Use `install_minotari_ledger.py` to install the Minotari Ledger Wallet app on
a supported Ledger device from a Tari GitHub release.

Supported devices:
- Ledger Nano S Plus
- Ledger Nano X
- Ledger Stax
- Ledger Flex

The original Ledger Nano S is not supported by the Minotari Ledger Wallet.

## Usage

From this directory:

```bash
python install_minotari_ledger.py
```

The installer detects the connected device, finds the newest non-draft Tari
release with a matching `minotari_ledger_wallet-<model>-*.zip` asset, verifies
the `.zip.sha256` sidecar, extracts the archive safely, and installs it.

To install a specific release:

```bash
python install_minotari_ledger.py --tag v5.4.0-pre.1
```

## Compatibility Wrappers

The existing per-model scripts remain as thin wrappers:

- `nanosplus/install_minotari_ledger_nanosplus.sh`
- `nanox/install_minotari_ledger_nanox.sh`
- `stax/install_minotari_ledger_stax.sh`
- `flex/install_minotari_ledger_flex.sh`
- `*/install_ledger_win.ps1`

They call the same auto-detecting installer so existing entry-point paths keep
working without bypassing device detection.

## Notes

- Python 3.9 or newer is required.
- The installer creates an isolated Python environment in the user cache and
  installs Ledger tooling there instead of modifying the system Python.
- Tari Ledger release archives must contain `minotari_ledger_wallet.apdu`,
  which is loaded through Ledger's secure APDU loader.
- Keep the Ledger connected, unlocked, and approve prompts on the device.
