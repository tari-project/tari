#!/usr/bin/env python3
"""
Unified cross-platform Ledger installer for Minotari Wallet.

Auto-detects connected Ledger model and installs the correct firmware.
Supports: Nano S Plus, Nano X, Stax, Flex

Usage:
    python3 install_minotari_ledger.py

Requirements:
    - Python 3.8+
    - Ledger device connected via USB
    - Device unlocked with Developer Mode enabled
"""

import sys
import os
import subprocess
import json
import tempfile
import shutil
import zipfile
import platform
from pathlib import Path
from typing import Optional, Dict, Tuple
from urllib.request import urlopen, Request
from urllib.error import HTTPError, URLError


# Ledger USB Vendor ID
LEDGER_VID = 0x2C97

# Ledger Product IDs by model
LEDGER_PRODUCT_IDS: Dict[int, str] = {
    0x0001: "nanos",       # Nano S (legacy, not supported for new installs)
    0x0005: "nanosplus",   # Nano S Plus
    0x0004: "nanox",       # Nano X
    0x0006: "stax",        # Stax
    0x0007: "flex",        # Flex
}

# Model display names
MODEL_NAMES: Dict[str, str] = {
    "nanos": "Nano S",
    "nanosplus": "Nano S Plus",
    "nanox": "Nano X",
    "stax": "Stax",
    "flex": "Flex",
}

# GitHub release configuration
GITHUB_REPO = "tari-project/tari"
GITHUB_API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"


def print_info(message: str) -> None:
    """Print info message."""
    print(f"ℹ️  {message}")


def print_success(message: str) -> None:
    """Print success message."""
    print(f"✅ {message}")


def print_error(message: str) -> None:
    """Print error message."""
    print(f"❌ {message}", file=sys.stderr)


def print_warning(message: str) -> None:
    """Print warning message."""
    print(f"⚠️  {message}")


def check_python_version() -> bool:
    """Check if Python version is 3.8 or higher."""
    if sys.version_info < (3, 8):
        print_error(f"Python 3.8+ required, found {sys.version_info.major}.{sys.version_info.minor}")
        return False
    return True


def install_dependencies() -> bool:
    """Install required Python packages."""
    required_packages = ["protobuf", "setuptools", "ecdsa", "ledgerwallet"]
    
    print_info("Checking Python dependencies...")
    
    for package in required_packages:
        try:
            __import__(package)
            print_success(f"{package} already installed")
        except ImportError:
            print_info(f"Installing {package}...")
            try:
                subprocess.check_call(
                    [sys.executable, "-m", "pip", "install", "-q", package],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL
                )
                print_success(f"{package} installed")
            except subprocess.CalledProcessError as e:
                print_error(f"Failed to install {package}: {e}")
                return False
    
    return True


def detect_ledger_hid() -> Optional[str]:
    """
    Detect Ledger device using HID USB enumeration.
    Returns the model slug or None if not found.
    """
    try:
        import hid
    except ImportError:
        print_warning("hid module not available, trying fallback detection")
        return None
    
    try:
        devices = hid.enumerate()
        for device in devices:
            if device.get("vendor_id") == LEDGER_VID:
                product_id = device.get("product_id", 0) & 0xFFFF
                if product_id in LEDGER_PRODUCT_IDS:
                    model = LEDGER_PRODUCT_IDS[product_id]
                    print_success(f"Detected Ledger {MODEL_NAMES.get(model, model)} via USB")
                    return model
    except Exception as e:
        print_warning(f"HID detection failed: {e}")
    
    return None


def detect_ledger_ledgerctl() -> Optional[str]:
    """
    Fallback: Detect Ledger device using ledgerctl info.
    Returns the model slug or None if not found.
    """
    try:
        result = subprocess.run(
            [sys.executable, "-m", "ledgerwallet", "info"],
            capture_output=True,
            text=True,
            timeout=10
        )
        
        if result.returncode != 0:
            return None
        
        output = result.stdout.lower()
        
        # Match full model names to avoid substring issues
        # Check longer names first to avoid partial matches
        if "nano s plus" in output:
            print_success("Detected Ledger Nano S Plus via ledgerctl")
            return "nanosplus"
        elif "nano x" in output:
            print_success("Detected Ledger Nano X via ledgerctl")
            return "nanox"
        elif "stax" in output:
            print_success("Detected Ledger Stax via ledgerctl")
            return "stax"
        elif "flex" in output:
            print_success("Detected Ledger Flex via ledgerctl")
            return "flex"
        elif "nano s" in output:
            print_warning("Detected Ledger Nano S (legacy, not supported)")
            return "nanos"
            
    except subprocess.TimeoutExpired:
        print_warning("ledgerctl info timed out")
    except FileNotFoundError:
        print_warning("ledgerwallet module not found")
    except Exception as e:
        print_warning(f"ledgerctl detection failed: {e}")
    
    return None


def detect_ledger_model() -> Optional[str]:
    """
    Detect connected Ledger model.
    Tries HID first, falls back to ledgerctl.
    """
    print_info("Detecting Ledger device...")
    
    # Try HID first (more reliable)
    model = detect_ledger_hid()
    if model:
        return model
    
    # Fallback to ledgerctl
    print_info("Trying fallback detection via ledgerctl...")
    model = detect_ledger_ledgerctl()
    if model:
        return model
    
    print_error("No Ledger device detected")
    print_info("Please ensure:")
    print_info("  • Ledger is connected via USB")
    print_info("  • Device is unlocked")
    print_info("  • Developer Mode is enabled (Settings > Developer)")
    return None


def fetch_latest_release() -> Dict:
    """Fetch latest release info from GitHub API."""
    print_info("Fetching latest release info from GitHub...")
    
    try:
        request = Request(
            GITHUB_API_URL,
            headers={
                "Accept": "application/vnd.github.v3+json",
                "User-Agent": "minotari-ledger-installer/1.0"
            }
        )
        
        with urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode("utf-8"))
    except HTTPError as e:
        print_error(f"GitHub API error: {e.code} - {e.reason}")
        raise
    except URLError as e:
        print_error(f"Network error: {e.reason}")
        raise
    except json.JSONDecodeError as e:
        print_error(f"Failed to parse GitHub response: {e}")
        raise


def find_asset_for_model(release_data: Dict, model: str) -> Optional[Dict]:
    """
    Find the correct asset for the detected Ledger model.
    
    Asset naming convention: minotari_ledger_wallet-{model}-v{version}-{hash}.zip
    Example: minotari_ledger_wallet-nanosplus-v5.3.0-rc.1-e8fc0e4.zip
    """
    assets = release_data.get("assets", [])
    
    # Look for asset matching the model
    for asset in assets:
        name = asset.get("name", "")
        
        # Match pattern: minotari_ledger_wallet-{model}-*.zip (but not .sha256)
        if name.startswith(f"minotari_ledger_wallet-{model}-") and name.endswith(".zip"):
            if not name.endswith(".sha256"):
                return asset
    
    return None


def download_file(url: str, dest_path: str, show_progress: bool = True) -> bool:
    """Download file with progress indicator."""
    try:
        request = Request(url, headers={"User-Agent": "minotari-ledger-installer/1.0"})
        
        with urlopen(request, timeout=120) as response:
            total_size = response.headers.get("Content-Length")
            
            if total_size:
                total_size = int(total_size)
                downloaded = 0
                chunk_size = 65536  # 64KB chunks
                
                with open(dest_path, "wb") as f:
                    while True:
                        chunk = response.read(chunk_size)
                        if not chunk:
                            break
                        f.write(chunk)
                        downloaded += len(chunk)
                        
                        if show_progress and total_size > 0:
                            percent = (downloaded / total_size) * 100
                            sys.stdout.write(f"\r   Progress: {percent:.1f}%")
                            sys.stdout.flush()
                
                if show_progress:
                    print()  # New line after progress
            else:
                # No content length, just download
                with open(dest_path, "wb") as f:
                    shutil.copyfileobj(response, f)
        
        return True
        
    except Exception as e:
        print_error(f"Download failed: {e}")
        return False


def download_firmware(model: str, release_data: Dict, temp_dir: str) -> Optional[str]:
    """Download firmware for the detected model."""
    asset = find_asset_for_model(release_data, model)
    
    if not asset:
        print_error(f"No firmware found for Ledger {MODEL_NAMES.get(model, model)}")
        print_info("Available assets:")
        for a in release_data.get("assets", []):
            name = a.get("name", "")
            if "ledger_wallet" in name and name.endswith(".zip"):
                print_info(f"  - {name}")
        return None
    
    url = asset["browser_download_url"]
    filename = asset["name"]
    
    print_info(f"Found firmware: {filename}")
    print_info(f"Downloading...")
    
    zip_path = os.path.join(temp_dir, filename)
    
    if not download_file(url, zip_path):
        return None
    
    print_success(f"Downloaded to {zip_path}")
    return zip_path


def extract_firmware(zip_path: str, extract_dir: str) -> Optional[str]:
    """Extract firmware zip and find app.json."""
    print_info("Extracting firmware...")
    
    try:
        with zipfile.ZipFile(zip_path, "r") as zip_ref:
            zip_ref.extractall(extract_dir)
        
        # Find app_*.json file
        for root, dirs, files in os.walk(extract_dir):
            for file in files:
                if file.startswith("app_") and file.endswith(".json"):
                    app_json_path = os.path.join(root, file)
                    print_success(f"Found app manifest: {file}")
                    return app_json_path
        
        print_error("No app_*.json found in firmware archive")
        return None
        
    except zipfile.BadZipFile:
        print_error("Invalid zip file")
        return None


def install_app(app_json_path: str) -> bool:
    """Install app using ledgerctl."""
    print_info("Installing app onto Ledger...")
    print_info("Please confirm on your Ledger device if prompted")
    
    try:
        result = subprocess.run(
            [sys.executable, "-m", "ledgerwallet", "install", app_json_path],
            capture_output=True,
            text=True,
            timeout=120
        )
        
        if result.returncode == 0:
            print_success("App installed successfully!")
            return True
        else:
            print_error(f"Installation failed: {result.stderr}")
            return False
            
    except subprocess.TimeoutExpired:
        print_error("Installation timed out")
        return False
    except Exception as e:
        print_error(f"Installation error: {e}")
        return False


def main() -> int:
    """Main entry point."""
    print("=" * 60)
    print("Minotari Ledger Wallet - Unified Installer")
    print("=" * 60)
    print()
    
    # Check Python version
    if not check_python_version():
        return 1
    
    # Install dependencies
    if not install_dependencies():
        print_error("Failed to install dependencies")
        return 1
    
    # Detect Ledger model
    model = detect_ledger_model()
    if not model:
        return 1
    
    # Check if model is supported
    if model == "nanos":
        print_error("Ledger Nano S is not supported")
        print_info("Supported models: Nano S Plus, Nano X, Stax, Flex")
        return 1
    
    print_info(f"Installing for Ledger {MODEL_NAMES.get(model, model)}")
    print()
    
    # Fetch release info
    try:
        release_data = fetch_latest_release()
        version = release_data.get("tag_name", "unknown")
        print_info(f"Latest release: {version}")
    except Exception:
        return 1
    
    # Download and install
    with tempfile.TemporaryDirectory() as temp_dir:
        # Download firmware
        zip_path = download_firmware(model, release_data, temp_dir)
        if not zip_path:
            return 1
        
        print()
        
        # Extract firmware
        extract_dir = os.path.join(temp_dir, "extracted")
        os.makedirs(extract_dir, exist_ok=True)
        
        app_json_path = extract_firmware(zip_path, extract_dir)
        if not app_json_path:
            return 1
        
        print()
        
        # Install app
        if not install_app(app_json_path):
            return 1
    
    print()
    print("=" * 60)
    print_success("Installation complete!")
    print("=" * 60)
    print()
    print("You can now use your Ledger with Minotari Wallet.")
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
