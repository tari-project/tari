# Minotari Ledger Nano S Plus Installer

This directory keeps compatibility entry points for Ledger Nano S Plus users.
Both scripts delegate to the unified installer in `../install_minotari_ledger.py`
with `--model nanosplus`.

## macOS / Linux

```bash
./install_minotari_ledger_nanosplus.sh
```

## Windows PowerShell

```powershell
.\install_ledger_win.ps1
```

Pass `--tag <release>` on macOS/Linux or `-Tag <release>` on Windows to install
a specific Tari release.
