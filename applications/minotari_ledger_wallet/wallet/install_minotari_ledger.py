#!/usr/bin/env python3
"""
Unified Minotari Ledger Installer

Cross-platform installer that auto-detects the connected Ledger model
(Nano S Plus, Nano X, Stax, Flex) and installs the correct Minotari
app — all in one step.

Supports macOS, Windows, and Linux.

Usage:
    python3 install_minotari_ledger.py [--tag TAG]
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

# Ledger target IDs per model
LEDGER_MODELS = {
    "nanosplus": {"target_id": "0x33100004", "name": "Nano S Plus"},
    "nanox":     {"target_id": "0x33000004", "name": "Nano X"},
    "stax":      {"target_id": "0x33200004", "name": "Stax"},
    "flex":      {"target_id": "0x33300004", "name": "Flex"},
}

GITHUB_REPO = "tari-project/tari"
GITHUB_API = f"https://api.github.com/repos/{GITHUB_REPO}"


def banner():
    print("=" * 50)
    print("  Minotari Ledger Installer")
    print("  Cross-platform • Auto-detect model")
    print("=" * 50)
    print()


def check_prerequisites():
    """Verify required tools are available."""
    errors = []

    if not shutil.which("python3") and not shutil.which("python"):
        errors.append("Python 3 is required but not found")

    try:
        import json as _  # noqa: F401
    except ImportError:
        errors.append("json module not available")

    # Check for ledgerblue (needed for installation)
    try:
        import ledgerblue  # noqa: F401
    except ImportError:
        errors.append("ledgerblue not found — install with: pip install ledgerblue")

    if errors:
        for e in errors:
            print(f"  ✗ {e}")
        sys.exit(1)

    print("  ✓ Prerequisites OK")


def detect_ledger_model():
    """
    Auto-detect the connected Ledger model.

    Tries multiple detection methods:
    1. ledgerctl list-devices (if available)
    2. USB device enumeration
    3. Ledger Live CLI
    """
    print("  🔍 Detecting connected Ledger...")

    # Method 1: Try ledgerctl
    try:
        result = subprocess.run(
            [sys.executable, "-m", "ledgerctl", "list-devices"],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode == 0:
            output = result.stdout.lower()
            for model_key, model_info in LEDGER_MODELS.items():
                if model_key in output or model_info["name"].lower() in output:
                    print(f"  ✓ Detected: {model_info['name']}")
                    return model_key
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass

    # Method 2: Try USB detection via lsusb (Linux) or system_profiler (macOS)
    system = platform.system().lower()
    try:
        if system == "linux":
            result = subprocess.run(
                ["lsusb"], capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                output = result.stdout.lower()
                if "ledger" in output:
                    # Try to identify model from USB product info
                    for model_key, model_info in LEDGER_MODELS.items():
                        if model_key.replace("nano ", "nano") in output:
                            print(f"  ✓ Detected: {model_info['name']}")
                            return model_key
        elif system == "darwin":
            result = subprocess.run(
                ["system_profiler", "SPUSBDataType"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                output = result.stdout.lower()
                if "ledger" in output:
                    for model_key, model_info in LEDGER_MODELS.items():
                        if model_info["name"].lower() in output:
                            print(f"  ✓ Detected: {model_info['name']}")
                            return model_key
        elif system == "windows":
            result = subprocess.run(
                ["powershell", "-Command",
                 "Get-PnpDevice -Class USB | Where-Object {$_.FriendlyName -like '*Ledger*'}"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                output = result.stdout.lower()
                for model_key, model_info in LEDGER_MODELS.items():
                    if model_info["name"].lower() in output:
                        print(f"  ✓ Detected: {model_info['name']}")
                        return model_key
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass

    # Method 3: Interactive selection
    print("  ⚠ Could not auto-detect Ledger model.")
    print("  Please select your Ledger model:")
    models = list(LEDGER_MODELS.items())
    for i, (key, info) in enumerate(models, 1):
        print(f"    {i}) {info['name']}")

    while True:
        try:
            choice = input("\n  Enter number (1-4): ").strip()
            idx = int(choice) - 1
            if 0 <= idx < len(models):
                selected = models[idx]
                print(f"  ✓ Selected: {selected[1]['name']}")
                return selected[0]
        except (ValueError, EOFError):
            pass
        print("  Invalid choice. Please enter 1-4.")


def fetch_latest_release(tag=None):
    """Fetch release info from GitHub API."""
    import urllib.request

    if tag:
        url = f"{GITHUB_API}/releases/tags/{tag}"
    else:
        url = f"{GITHUB_API}/releases/latest"

    print(f"  🌐 Fetching release info...")
    req = urllib.request.Request(url, headers={
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "minotari-ledger-installer"
    })
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read())


def find_asset(release, model_key):
    """Find the correct download asset for the given model."""
    pattern = f"minotari_ledger_wallet-{model_key}"
    for asset in release.get("assets", []):
        name = asset.get("name", "")
        if pattern in name and name.endswith(".zip"):
            return asset["browser_download_url"], name
    return None, None


def download_asset(url, dest_dir):
    """Download a file with progress."""
    import urllib.request

    filename = url.split("/")[-1]
    dest = dest_dir / filename

    print(f"  ⬇️  Downloading {filename}...")

    def progress(block_num, block_size, total_size):
        downloaded = block_num * block_size
        if total_size > 0:
            pct = min(100, downloaded * 100 // total_size)
            print(f"\r  ⬇️  {pct}% ({downloaded // 1024}KB)", end="", flush=True)

    try:
        urllib.request.urlretrieve(url, str(dest), reporthook=progress)
    except Exception as e:
        print(f"\n  ✗ Download failed: {e}")
        return None
    print()
    return dest


def install_on_ledger(model_key, zip_path, target_id):
    """Install the app onto the Ledger device."""
    print(f"  📦 Extracting firmware...")
    extract_dir = zip_path.parent / "extracted"
    extract_dir.mkdir(exist_ok=True)

    with zipfile.ZipFile(zip_path, "r") as zf:
        # Prevent path traversal attacks
        for member in zf.namelist():
            member_path = (extract_dir / member).resolve()
            if not str(member_path).startswith(str(extract_dir.resolve())):
                print(f"  ✗ Refusing to extract path traversal: {member}")
                return False
        zf.extractall(extract_dir)

    # Find the .apdu install script
    apdu_files = list(extract_dir.rglob("*.apdu"))
    if not apdu_files:
        print("  ✗ No .apdu install script found in the archive.")
        print("    The firmware archive may have an unexpected structure.")
        return False

    apdu_file = apdu_files[0]
    print(f"  ✓ Found install script: {apdu_file.name}")

    print()
    print("  🔐 Installing app onto Ledger...")
    print("  👉 Ensure:")
    print("     • Ledger connected via USB")
    print("     • Device unlocked")
    print("     • Developer Mode enabled")
    print()

    # Remove any previous install
    try:
        subprocess.run(
            [sys.executable, "-m", "ledgerctl", "delete", "MinoTari Wallet"],
            capture_output=True, timeout=10
        )
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass

    # Install via ledgerblue
    try:
        result = subprocess.run(
            [
                sys.executable, "-m", "ledgerblue.runScript",
                "--targetId", target_id,
                "--fileName", str(apdu_file),
                "--apdu", "--scp"
            ],
            capture_output=True, text=True, timeout=120
        )
        if result.returncode == 0:
            return True
        else:
            print(f"  ✗ Install failed: {result.stderr}")
            return False
    except FileNotFoundError:
        print("  ✗ ledgerblue not found. Install it: pip install ledgerblue")
        return False
    except subprocess.TimeoutExpired:
        print("  ✗ Install timed out")
        return False


def main():
    banner()

    parser = argparse.ArgumentParser(
        description="Unified Minotari Ledger Installer"
    )
    parser.add_argument(
        "-t", "--tag",
        help="Install a specific release tag (e.g. v5.2.0-pre.7)"
    )
    args = parser.parse_args()

    print("  Checking prerequisites...")
    check_prerequisites()
    print()

    model_key = detect_ledger_model()
    model_info = LEDGER_MODELS[model_key]
    print()

    release = fetch_latest_release(args.tag)
    version = release.get("tag_name", "unknown")
    print(f"  ✓ Release: {version}")
    print()

    asset_url, asset_name = find_asset(release, model_key)
    if not asset_url:
        print(f"  ✗ No release asset found for {model_info['name']}")
        print(f"    Looked for: minotari_ledger_wallet-{model_key}*.zip")
        return 1

    with tempfile.TemporaryDirectory() as tmp_dir:
        tmp_path = Path(tmp_dir)
        zip_path = download_asset(asset_url, tmp_path)
        if zip_path is None:
            return 1
        print()

        success = install_on_ledger(model_key, zip_path, model_info["target_id"])

    print()
    if success:
        print("  ✅ Minotari Ledger Wallet installed successfully!")
        print(f"     Model: {model_info['name']}")
        print(f"     Version: {version}")
        return 0
    else:
        print("  ❌ Installation failed. Check the output above.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
