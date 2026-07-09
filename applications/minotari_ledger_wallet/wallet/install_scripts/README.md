# Minotari Ledger Wallet Installer

Use the launcher for your platform to install the Minotari Ledger Wallet app on
a supported Ledger device from a Tari GitHub release.

Supported devices:
- Ledger Nano S Plus
- Ledger Nano X
- Ledger Stax
- Ledger Flex

The original Ledger Nano S is not supported by the Minotari Ledger Wallet.

## Usage

From this directory on macOS or Linux:

```bash
./install_minotari_ledger.sh
```

From this directory on Windows PowerShell:

```powershell
.\install_minotari_ledger.ps1
```

The launcher checks for Python 3.9 or newer and prompts before running a
platform package-manager command if Python is missing. The Python installer then
detects the connected device, finds the newest non-draft Tari release with a
matching `minotari_ledger_wallet-<model>-*.zip` asset, verifies the `.zip.sha256`
sidecar, extracts the archive safely, and installs it.

To install a specific release:

```bash
./install_minotari_ledger.sh --tag v5.4.0-pre.1
```

```powershell
.\install_minotari_ledger.ps1 -Tag v5.4.0-pre.1
```

## Direct Python Usage

If Python 3.9 or newer is already available, the installer can also be run
directly:

```bash
python install_minotari_ledger.py
python install_minotari_ledger.py --tag v5.4.0-pre.1
```

There are no model-specific scripts. The installer always auto-detects the
connected Ledger model.

## Notes

- The installer creates an isolated Python environment in the user cache and
  installs Ledger tooling there instead of modifying the system Python.
- Tari Ledger release archives must contain `minotari_ledger_wallet.apdu`,
  which is loaded through Ledger's secure APDU loader.
- Keep the Ledger connected, unlocked, and approve prompts on the device.
