#!/usr/bin/env python3
"""
Minotari Ledger Wallet - Unified Cross-Platform Installer

Auto-detects connected Ledger model (Nano S, Nano S Plus, Nano X, Stax, Flex),
downloads the correct binary from GitHub releases, and installs it.

Supports: macOS, Windows, Linux
Requires: Python 3.8+, pip
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional


# ── USB Vendor/Product IDs for Ledger devices ─────────────────────────────────
LEDGER_USB_IDS = {
    ("03eb", "2402"): "nanos",       # Ledger Nano S
    ("2c97", "0001"): "nanos",       # Ledger Nano S (alt ID)
    ("2c97", "0004"): "nanos",       # Ledger Nano S (alt ID)
    ("03eb", "2404"): "nanox",       # Ledger Nano X
    ("2c97", "0005"): "nanox",       # Ledger Nano X (alt ID)
    ("2c97", "0002"): "nanox",       # Ledger Nano X (alt ID)
    ("03eb", "2405"): "nanosplus",  # Ledger Nano S Plus
    ("2c97", "000a"): "nanosplus",  # Ledger Nano S Plus (alt ID)
    ("03eb", "2406"): "stax",       # Ledger Stax
    ("2c97", "0003"): "stax",        # Ledger Stax (alt ID)
    ("03eb", "2407"): "flex",       # Ledger Flex
    ("2c97", "0020"): "flex",       # Ledger Flex (alt ID)
}

# GitHub release asset name patterns per model
ASSET_PATTERNS = {
    "nanos":    r"minotari_ledger_wallet-nanos.*\.zip$",
    "nanox":    r"minotari_ledger_wallet-nanox.*\.zip$",
    "nanosplus":r"minotari_ledger_wallet-nanosplus.*\.zip$",
    "stax":     r"minotari_ledger_wallet-stax.*\.zip$",
    "flex":     r"minotari_ledger_wallet-flex.*\.zip$",
}

REPO_OWNER = "tari-project"
REPO_NAME  = "tari"
BINARY_NAME = "Minotari Wallet"


class OS(Enum):
    MACOS   = "macos"
    LINUX   = "linux"
    WINDOWS = "windows"
    UNKNOWN = "unknown"


def detect_os() -> OS:
    if sys.platform.startswith("darwin"):
        return OS.MACOS
    elif sys.platform.startswith("linux"):
        return OS.LINUX
    elif sys.platform.startswith("win"):
        return OS.WINDOWS
    return OS.UNKNOWN


def run_cmd(cmd: list[str], check: bool = True, capture: bool = True) -> tuple[int, str, str]:
    """
    Run a command, return (returncode, stdout, stderr).
    FIXED: Removed broken Windows shell=True logic.
    """
    try:
        kwargs = {"capture_output": True, "text": True, "shell": False}
        result = subprocess.run(cmd, **kwargs)
        return result.returncode, result.stdout or "", result.stderr or ""
    except Exception as e:
        return -1, "", str(e)


def check_python_min_version(major: int = 3, minor: int = 8) -> bool:
    return sys.version_info >= (major, minor)


def ensure_python_deps() -> bool:
    """
    Check (not auto-install) required Python packages.
    FIXED: Made dependency management less intrusive (check first, ask user).
    Returns True if all deps are available.
    """
    required = ["ledgerwallet", "ecdsa", "protobuf"]
    
    # Check which packages are missing
    missing = []
    for pkg in required:
        try:
            __import__(pkg.replace("-", "_"))
        except ImportError:
            missing.append(pkg)
    
    if not missing:
        return True  # All deps already installed
    
    print(f"[*] Missing Python packages: {', '.join(missing)}")
    print(f"[*] Install them with: pip install {' '.join(missing)}")
    print(f"[*] Or run with --auto-install to install automatically.")
    
    # Check for --auto-install flag
    if "--auto-install" in sys.argv:
        print(f"[*] Auto-installing missing packages...")
        pip_cmd = [sys.executable, "-m", "pip", "install", "--upgrade"] + missing
        rc, out, err = run_cmd(pip_cmd)
        if rc != 0:
            print(f"[!] Failed to install Python deps: {err}")
            return False
        return True
    
    print(f"[!] Please install missing packages manually, then re-run.")
    return False


def get_github_release(tag: Optional[str] = None) -> dict:
    """Fetch GitHub release info (latest or specific tag)."""
    if tag:
        url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/tags/{tag}"
    else:
        url = f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"

    req = urllib.request.Request(url, headers={"Accept": "application/vnd.github.v3+json"})
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return json.loads(resp.read())
    except Exception as e:
        raise RuntimeError(f"Failed to fetch GitHub release: {e}")


def find_asset_url(release: dict, model: str) -> str:
    """Find the download URL for the asset matching the model."""
    import re
    pattern = ASSET_PATTERNS.get(model)
    if not pattern:
        raise ValueError(f"Unknown model: {model}")

    for asset in release.get("assets", []):
        if re.search(pattern, asset["name"], re.IGNORECASE):
            return asset["browser_download_url"]

    raise RuntimeError(
        f"No release asset found for model '{model}'. "
        f"Check https://github.com/{REPO_OWNER}/{REPO_NAME}/releases"
    )


def detect_ledger_model() -> Optional[str]:
    """
    Detect connected Ledger model via USB HID IDs.
    Falls back to ledgerwallet Python library if lsusb/hidutil not available.
    FIXED: Bugs in macOS USB detection routine.
    """
    system = detect_os()

    # ── macOS: use system_profiler / hidutil ──────────────────────────────────
    if system == OS.MACOS:
        # Try system_profiler (requires macOS 10.15+)
        rc, out, _ = run_cmd(["system_profiler", "SPUSBDataType", "-json"])
        if rc == 0 and out:
            try:
                data = json.loads(out)
                for device in data.get("SPUSBDataType", []):
                    # FIXED: Check both "_name" and "name" fields
                    name = device.get("_name", "") or device.get("name", "")
                    if "Ledger" in name:
                        vid = device.get("vendor_id", "") or device.get("Vendor ID", "")
                        pid = device.get("product_id", "") or device.get("Product ID", "")
                        # Normalize: remove "0x" prefix, lowercase
                        vid = vid.replace("0x", "").lower().zfill(4)
                        pid = pid.replace("0x", "").lower().zfill(4)
                        key = (vid, pid)
                        if key in LEDGER_USB_IDS:
                            return LEDGER_USB_IDS[key]
            except Exception:
                pass

        # Try hidutil for direct USB info
        rc, out, _ = run_cmd(["hidutil", "list"])
        if rc == 0:
            for vid_pid, model_name in LEDGER_USB_IDS.items():
                vid, pid = vid_pid
                if vid in out.lower() and pid in out.lower():
                    return model_name

    # ── Linux: use lsusb ──────────────────────────────────────────────────────
    elif system == OS.LINUX:
        rc, out, _ = run_cmd(["lsusb"])
        if rc == 0:
            for line in out.splitlines():
                if "Ledger" in line:
                    # Format: Bus 001 Device 002: ID 2c97:0005 Ledger ...
                    if "ID" in line:
                        parts = line.split()
                        id_part = parts[parts.index("ID") + 1]
                        if ":" in id_part:
                            vid, pid = id_part.split(":")
                            vid = vid.lower().zfill(4)
                            pid = pid.lower().zfill(4)
                            key = (vid, pid)
                            if key in LEDGER_USB_IDS:
                                return LEDGER_USB_IDS[key]

    # ── Windows: use PowerShell ────────────────────────────────────────────────
    elif system == OS.WINDOWS:
        # Try PowerShell PnPDevice query
        ps_script = r'''
        Get-PnpDevice -Class "HIDClass" -Status OK |
          Where-Object { $_.FriendlyName -like "*Ledger*" } |
          Select-Object InstanceId |
          ConvertTo-Json -Compress
        '''
        rc, out, _ = run_cmd(["powershell", "-Command", ps_script])
        if rc == 0 and out:
            try:
                data = json.loads(out)
                # Parse VID/PID from InstanceId e.g. "HID\\VID_2C97&PID_0005..."
                for vid_pid, model_name in LEDGER_USB_IDS.items():
                    vid, pid = vid_pid
                    vid_clean = vid.replace(":", "").lower()
                    pid_clean = pid.replace(":", "").lower()
                    if vid_clean in out.lower() and pid_clean in out.lower():
                        return model_name
            except Exception:
                pass

        # Try device path parsing via wmic
        rc, out, _ = run_cmd(
            ["wmic", "path", "Win32_USBControllerDevice", "get", "/format:csv"]
        )
        if rc == 0:
            for line in out.splitlines():
                if "ledger" in line.lower():
                    for vid_pid, model_name in LEDGER_USB_IDS.items():
                        vid, pid = vid_pid
                        if vid in line.lower() and pid in line.lower():
                            return model_name

    # ── Universal: try ledgerwallet Python lib ────────────────────────────────
    try:
        from ledgerwallet.ledger import enumerate_devices
        from ledgerwallet.cards import LedgerWallet
        devices = enumerate_devices()
        for dev in devices:
            # Get device info
            try:
                # Try to get the target ID from the device
                target_id = dev.target_id if hasattr(dev, 'target_id') else None
                if target_id:
                    # Map target_id to model
                    TARGET_ID_MAP = {
                        0x31000001: "nanos",
                        0x31000002: "nanox",
                        0x31000003: "nanosplus",
                        0x31000004: "stax",
                        0x31000005: "flex",
                    }
                    for tid, model_name in TARGET_ID_MAP.items():
                        if dev.target_id == tid:
                            return model_name
            except Exception:
                pass
    except ImportError:
        pass

    return None


def ensure_ledgerctl() -> bool:
    """Ensure ledgerctl is installed and in PATH."""
    rc, _, _ = run_cmd(["ledgerctl", "--version"], check=False)
    if rc == 0:
        return True

    # Install ledgerctl
    print("[*] ledgerctl not found, installing...")
    rc, _, err = run_cmd(
        [sys.executable, "-m", "pip", "install", "git+https://github.com/LedgerHQ/ledgerctl"]
    )
    if rc != 0:
        print(f"[!] Failed to install ledgerctl: {err}")
        return False
    return True


def download_and_install(model: str, release_tag: Optional[str] = None) -> bool:
    """
    Download the correct binary and install to Ledger.
    FIXED: Use try-finally block for robust cleanup of temporary directories.
    """
    print(f"[*] Fetching release info from GitHub...")
    try:
        release = get_github_release(release_tag)
        asset_url = find_asset_url(release, model)
        version = release.get("tag_name", "unknown")
        print(f"[*] Version: {version}")
        print(f"[*] Model: {model}")
        print(f"[*] Asset: {asset_url}")
    except Exception as e:
        print(f"[!] {e}")
        return False

    # Download to temp dir
    tmp_dir = tempfile.mkdtemp(prefix="tari_ledger_")
    
    # FIXED: Use try-finally to ensure cleanup
    try:
        zip_path = os.path.join(tmp_dir, f"minotari_{model}.zip")

        print(f"[*] Downloading...")
        try:
            urllib.request.urlretrieve(asset_url, zip_path)
        except Exception as e:
            print(f"[!] Download failed: {e}")
            return False

        # Extract zip
        print(f"[*] Extracting...")
        app_json = None
        try:
            with zipfile.ZipFile(zip_path, "r") as z:
                z.extractall(tmp_dir)
            # Find the app JSON file
            import re
            pattern = ASSET_PATTERNS[model]
            for asset in release.get("assets", []):
                if re.search(pattern, asset["name"], re.IGNORECASE):
                    base = asset["name"].replace(".zip", "")
                    # The JSON inside has model-specific name
                    model_json = {
                        "nanos":    "app.json",     # Check actual filenames
                        "nanox":    "app.json",
                        "nanosplus":"app.json",
                        "stax":     "app.json",
                        "flex":     "app.json",
                    }
                    # Find the app JSON
                    for fname in os.listdir(tmp_dir):
                        if fname.endswith(".json") and "app" in fname.lower():
                            app_json = os.path.join(tmp_dir, fname)
                            break
                    break
        except Exception as e:
            print(f"[!] Extraction failed: {e}")
            return False

        if not app_json or not os.path.exists(app_json):
            print(f"[!] app.json not found in extracted files")
            return False

        # Ensure ledgerctl is available
        if not ensure_ledgerctl():
            return False

        # Run ledgerctl install
        model_display = {
            "nanos":    "Nano S",
            "nanox":    "Nano X",
            "nanosplus":"Nano S Plus",
            "stax":     "Stax",
            "flex":     "Flex",
        }
        print(f"\n[*] Installing Minotari Wallet on Ledger {model_display.get(model, model)}...")
        print(f"[*] Ensure:")
        print(f"    - Ledger {model_display.get(model, model)} connected via USB")
        print(f"    - Device unlocked")
        print(f"    - Developer Mode / Ledger Live ready")
        print()

        rc, stdout, stderr = run_cmd(["ledgerctl", "install", app_json])
        print(stdout)
        if stderr:
            print(stderr, file=sys.stderr)

        if rc == 0:
            print(f"\n[+] Minotari Ledger Wallet installed successfully!")
            return True
        else:
            print(f"\n[!] Installation failed (exit code {rc})")
            return False
    finally:
        # FIXED: Robust cleanup using try-finally
        if os.path.exists(tmp_dir):
            shutil.rmtree(tmp_dir, ignore_errors=True)

def main():
    if not check_python_min_version(3, 8):
        print("[!] Python 3.8+ required")
        sys.exit(1)

    parser = argparse.ArgumentParser(
        description="Install Minotari Ledger Wallet - Unified Cross-Platform Installer",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Auto-detect and install (latest release)
  python install_minotari_ledger.py

  # Install a specific GitHub release tag
  python install_minotari_ledger.py --tag v5.2.0

  # Force model selection (skip auto-detection)
  python install_minotari_ledger.py --model nanox

  # Auto-install missing Python dependencies
  python install_minotari_ledger.py --auto-install

Supported models: nanos, nanox, nanosplus, stax, flex
Supported OS: macOS, Linux, Windows
"""
    )
    parser.add_argument("-t", "--tag", dest="tag", default=None,
                        help="Install a specific release tag (e.g. v5.2.0-pre.7)")
    parser.add_argument("-m", "--model", dest="model", default=None,
                        choices=["nanos", "nanox", "nanosplus", "stax", "flex"],
                        help="Force model (skip auto-detection)")
    parser.add_argument("--auto-install", dest="auto_install", action="store_true",
                        help="Auto-install missing Python dependencies (intrusive)")

    args = parser.parse_args()

    # FIXED: Pass --auto-install flag to ensure_python_deps()
    if args.auto_install:
        sys.argv.append("--auto-install")  # Hack to pass flag to function

    system = detect_os()
    if system == OS.UNKNOWN:
        print(f"[!] Unsupported OS: {sys.platform}")
        sys.exit(1)
    print(f"[*] Detected OS: {system.value}")

    # Ensure Python deps
    print("[*] Checking Python dependencies...")
    if not ensure_python_deps():
        sys.exit(1)

    # Detect or use forced model
    model = args.model
    if not model:
        print("[*] Detecting connected Ledger device...")
        model = detect_ledger_model()
        if not model:
            print("[!] No Ledger device detected.")
            print("[!] Please connect your Ledger device and try again.")
            print("[!] Supported devices: Nano S, Nano X, Nano S Plus, Stax, Flex")
            sys.exit(1)
        print(f"[+] Detected: Ledger {model}")

    # Download and install
    success = download_and_install(model, args.tag)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
