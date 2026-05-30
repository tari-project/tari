# Minotari Ledger Nano X Installer

This directory keeps compatibility entry points for Ledger Nano X users.
Both scripts delegate to the auto-detecting unified installer in
`../install_minotari_ledger.py`.

## macOS / Linux

```bash
./install_minotari_ledger_nanox.sh
```

## Windows PowerShell

```powershell
.\install_ledger_win.ps1
```

Pass `--tag <release>` on macOS/Linux or `-Tag <release>` on Windows to install
a specific Tari release.
