#!/usr/bin/env python3
"""
Unified Minotari Ledger Wallet Installer
=========================================
Auto-detects the connected Ledger model and installs the correct Minotari app.

Supports: Nano S Plus, Nano X, Stax, Flex
Works on: macOS, Linux, Windows

Usage:
    python3 install_minotari_ledger.py [--tag TAG]

Requirements:
    pip install protobuf setuptools ecdsa ledgerwallet ledgerblue ledgerctl
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

# ── Device registry ──────────────────────────────────────────────────────────
# targetId values from Ledger's BOLOS SDK / cargo-ledger
DEVICES = {
    "0x33100004": {
        "name": "Nano S Plus",
        "asset_pattern": "minotari_ledger_wallet-nanosplus",
        "target_id": "0x33100004",
    },
    "0x33000004": {
        "name": "Nano X",
        "asset_pattern": "minotari_ledger_wallet-nanox",
        "target_id": "0x33000004",
    },
    "0x33200004": {
        "name": "Stax",
        "asset_pattern": "minotari_ledger_wallet-stax",
        "target_id": "0x33200004",
    },
    "0x33300004": {
        "name": "Flex",
        "asset_pattern": "minotari_ledger_wallet-flex",
        "target_id": "0x33300004",
    },
}

RELEASE_API_LATEST = "https://api.github.com/repos/tari-project/tari/releases/latest"
RELEASE_API_TAG = "https://api.github.com/repos/tari-project/tari/releases/tags/{tag}"
DOWNLOAD_DIR_NAME = "tari-downloads"
VENV_DIR_NAME = "tari-ledger-live"


# ── Helpers ──────────────────────────────────────────────────────────────────

def run(cmd, check=True, capture=False, **kwargs):
    """Run a subprocess, with optional output capture."""
    result = subprocess.run(
        cmd,
        shell=isinstance(cmd, str),
        check=check,
        capture_output=capture,
        text=True,
        **kwargs,
    )
    return result


def pip_install(packages):
    """Install Python packages into the current environment."""
    run([sys.executable, "-m", "pip", "install", "--quiet", "--upgrade", "pip"])
    run([sys.executable, "-m", "pip", "install", "--quiet"] + packages)


def ensure_dependencies():
    """Make sure all required Python packages are installed."""
    required = ["protobuf", "setuptools", "ecdsa", "ledgerwallet", "ledgerblue"]
    missing = []
    for pkg in required:
        try:
            __import__(pkg.replace("-", "_"))
        except ImportError:
            missing.append(pkg)

    if missing:
        print(f"📦 Installing Python dependencies: {', '.join(missing)}")
        pip_install(missing)

    # ledgerctl is a CLI tool, check separately
    if not shutil.which("ledgerctl"):
        print("📦 Installing ledgerctl...")
        pip_install(["ledgerctl"])


def detect_device():
    """
    Detect the connected Ledger device using the ledgerwallet library.
    Returns (target_id, device_info_dict) or raises RuntimeError.
    """
    try:
        from ledgerwallet.client import LedgerClient
        from ledgerwallet.transport.hid import HidTransport
    except ImportError:
        raise RuntimeError(
            "ledgerwallet not installed. Run: pip install ledgerwallet"
        )

    # Try to enumerate HID devices
    try:
        devices = HidTransport.enumerate()
    except Exception as e:
        raise RuntimeError(f"Failed to enumerate HID devices: {e}")

    if not devices:
        raise RuntimeError(
            "No Ledger device detected.\n"
            "  • Make sure the device is connected via USB\n"
            "  • Unlock the device (enter PIN)\n"
            "  • Enable Developer Mode in Settings"
        )

    # Use the first detected device
    dev = devices[0]

    # Get the target ID from the device
    # The transport gives us the HID device; we need to query the target ID
    # via the APDU GET_DEVICE_INFO command
    try:
        transport = HidTransport(dev)
        client = LedgerClient(transport)

        # APDU: CLA=0xE0, INS=0x01 (GET_DEVICE_INFO), P1=0x00, P2=0x00
        response = client.apdu_exchange(
            cla=0xE0, ins=0x01, p1=0x00, p2=0x00, data=b""
        )

        # Parse response: target_id is at bytes 0-4 (big-endian uint32)
        if len(response) >= 4:
            target_id_int = int.from_bytes(response[0:4], "big")
            target_id = f"0x{target_id_int:08X}"
        else:
            raise RuntimeError("Unexpected device info response length")

        transport.close()
        return target_id

    except RuntimeError:
        raise
    except Exception as e:
        raise RuntimeError(f"Failed to query device info: {e}")


def get_release_asset_url(release_api_url, asset_pattern):
    """Fetch the release and find the matching asset download URL."""
    req = urllib.request.Request(
        release_api_url,
        headers={"User-Agent": "minotari-ledger-installer/1.0"},
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        release = json.loads(resp.read())

    for asset in release.get("assets", []):
        name = asset["name"]
        if asset_pattern in name and name.endswith(".zip"):
            return asset["browser_download_url"], name

    return None, None


def download_file(url, dest_path):
    """Download a file with progress indication."""
    print(f"⬇️  Downloading: {url}")
    urllib.request.urlretrieve(url, dest_path)
    size_mb = os.path.getsize(dest_path) / (1024 * 1024)
    print(f"   Saved ({size_mb:.1f} MB)")


def extract_apdu_script(zip_path, extract_dir):
    """Extract the zip and find the .apdu install script."""
    with zipfile.ZipFile(zip_path, "r") as zf:
        zf.extractall(extract_dir)

    for root, _, files in os.walk(extract_dir):
        for f in files:
            if f.endswith(".apdu"):
                return os.path.join(root, f)

    return None


def install_onto_ledger(apdu_path, target_id):
    """Install the app onto the Ledger device using ledgerblue."""
    print()
    print("🔐 Installing app onto Ledger...")
    print("   • Ledger connected via USB")
    print("   • Device unlocked")
    print("   • Developer Mode enabled")
    print()

    # Remove previous install (best effort)
    try:
        run(["ledgerctl", "delete", "MinoTari Wallet"], check=False, capture=True)
    except Exception:
        pass

    # Run the install script
    run([
        sys.executable, "-m", "ledgerblue.runScript",
        "--targetId", target_id,
        "--fileName", apdu_path,
        "--apdu", "--scp",
    ])


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Unified Minotari Ledger Wallet Installer"
    )
    parser.add_argument(
        "--tag", "-t",
        default=None,
        help="Install a specific release tag (e.g. v5.2.0-pre.7). Defaults to latest.",
    )
    parser.add_argument(
        "--device", "-d",
        default=None,
        choices=["nanosplus", "nanox", "stax", "flex"],
        help="Override auto-detection and install for a specific device.",
    )
    args = parser.parse_args()

    print("=" * 60)
    print("  Minotari Ledger Wallet — Unified Installer")
    print("=" * 60)
    print()

    # Step 1: Ensure dependencies
    print("── Step 1: Checking dependencies ──")
    ensure_dependencies()
    print()

    # Step 2: Detect device
    print("── Step 2: Detecting Ledger device ──")
    if args.device:
        # Manual override
        device_key_map = {
            "nanosplus": "0x33100004",
            "nanox": "0x33000004",
            "stax": "0x33200004",
            "flex": "0x33300004",
        }
        target_id = device_key_map[args.device]
        print(f"   Manual override: {DEVICES[target_id]['name']}")
    else:
        try:
            target_id = detect_device()
        except RuntimeError as e:
            print(f"❌ {e}")
            sys.exit(1)

    device_info = DEVICES.get(target_id)
    if not device_info:
        print(f"❌ Unsupported device (target ID: {target_id})")
        print("   Supported devices: Nano S Plus, Nano X, Stax, Flex")
        sys.exit(1)

    print(f"   Detected: {device_info['name']} ({target_id})")
    print()

    # Step 3: Fetch release info
    print("── Step 3: Fetching release info ──")
    if args.tag:
        release_url = RELEASE_API_TAG.format(tag=args.tag)
        print(f"   Tag: {args.tag}")
    else:
        release_url = RELEASE_API_LATEST
        print("   Using latest release")

    asset_url, asset_name = get_release_asset_url(
        release_url, device_info["asset_pattern"]
    )
    if not asset_url:
        print(f"❌ Could not find release asset for {device_info['name']}")
        sys.exit(1)
    print(f"   Found: {asset_name}")
    print()

    # Step 4: Download
    print("── Step 4: Downloading ──")
    download_dir = Path.home() / "src" / "tari" / DOWNLOAD_DIR_NAME
    download_dir.mkdir(parents=True, exist_ok=True)
    zip_path = download_dir / asset_name
    download_file(asset_url, str(zip_path))
    print()

    # Step 5: Extract
    print("── Step 5: Extracting ──")
    apdu_path = extract_apdu_script(str(zip_path), str(download_dir))
    if not apdu_path:
        print("❌ Could not find .apdu install script in the archive")
        sys.exit(1)
    print(f"   Found: {apdu_path}")
    print()

    # Step 6: Install
    print("── Step 6: Installing ──")
    try:
        install_onto_ledger(apdu_path, device_info["target_id"])
    except subprocess.CalledProcessError as e:
        print(f"❌ Installation failed: {e}")
        sys.exit(1)

    print()
    print("=" * 60)
    print(f"  ✅ Minotari Ledger Wallet installed on {device_info['name']}!")
    print("=" * 60)


if __name__ == "__main__":
    main()
