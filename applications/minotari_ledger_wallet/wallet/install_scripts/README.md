# Minotari Ledger Installer

Use the unified installer from this directory:

```bash
python3 install_minotari_ledger.py
```

On Windows:

```powershell
python install_minotari_ledger.py
```

The installer:

- detects one connected Ledger device over USB
- supports Nano S Plus, Nano X, Stax, and Flex
- finds the newest non-draft Tari release that contains the matching Minotari Ledger wallet asset, including pre-releases
- downloads the matching zip and `.sha256` sidecar
- verifies the checksum before extraction
- safely extracts the archive
- installs the `.apdu` script with `ledgerblue.runScript --scp`

Before running the final install, connect exactly one Ledger device, unlock it, and leave it on the dashboard.

Useful options:

```bash
python3 install_minotari_ledger.py --dry-run
python3 install_minotari_ledger.py --model nanosplus
python3 install_minotari_ledger.py --tag v5.4.0-pre.2
python3 install_minotari_ledger.py --download-dir ./ledger-downloads
```

The legacy model-specific shell and PowerShell scripts remain as compatibility wrappers around this installer.
