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

The launcher makes sure [Astral's `uv`](https://docs.astral.sh/uv/) is available
(installing it from <https://astral.sh/uv> if needed) and then runs the installer
with `uv run`. `uv` provisions an isolated Python and the required Ledger tooling
on demand — **no system Python or pip setup is required.** The installer then
detects the connected device, finds the newest non-draft Tari release with a
matching `minotari_ledger_wallet-<model>-*.zip` asset, verifies the `.zip.sha256`
sidecar, extracts the archive safely, removes any previous install of the app,
and installs the new one.

To install a specific release:

```bash
./install_minotari_ledger.sh --tag v5.4.0-pre.1
```

```powershell
.\install_minotari_ledger.ps1 -Tag v5.4.0-pre.1
```

## Direct Usage

If `uv` is already installed, the installer can be run directly:

```bash
uv run install_minotari_ledger.py
uv run install_minotari_ledger.py --tag v5.4.0-pre.1
```

Running `python install_minotari_ledger.py` directly also works, but only if the
`ledgerwallet` and `ledgerblue` packages are already installed in that Python
environment. Using `uv run` (or the launchers) is recommended so those
dependencies are provisioned automatically.

There are no model-specific scripts. The installer always auto-detects the
connected Ledger model.

## Notes

- `uv` runs the installer in an ephemeral, isolated environment; nothing is
  installed into or modified in your system Python.
- Before flashing, the installer removes any existing `MinoTari Wallet` app so a
  reinstall or upgrade does not clash with a prior install. This is best-effort:
  a device with no prior install is not an error.
- Tari Ledger release archives must contain `minotari_ledger_wallet.apdu`,
  which is loaded through Ledger's secure APDU loader.
- Keep the Ledger connected, unlocked, and approve prompts on the device.
- Set the `NO_COLOR` environment variable to disable coloured output.
