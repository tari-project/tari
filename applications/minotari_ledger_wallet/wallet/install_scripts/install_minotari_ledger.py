# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "ledgerwallet",
#     "ledgerblue",
#     "requests"
# ]
# ///

import sys
import os
import subprocess
import requests
import zipfile
import tempfile
import shutil
import argparse
from urllib.parse import urlparse
from ledgerwallet.transport import enumerate_devices

# ---------------------------------------------------------------------------
# Ledger USB Product IDs to Tari Slug Mapping
# ---------------------------------------------------------------------------
LEDGER_VENDOR_ID = 0x2C97

def print_header(msg):
    print(f"\n\033[1;36m==>\033[0m \033[1m{msg}\033[0m")

def print_success(msg):
    print(f"\033[1;32m==>\033[0m \033[1m{msg}\033[0m")

def print_error(msg):
    print(f"\n\033[1;31m==> ERROR:\033[0m \033[1m{msg}\033[0m", file=sys.stderr)

def print_warning(msg):
    print(f"\033[1;33m==> WARNING:\033[0m \033[1m{msg}\033[0m")

def detect_device():
    print_header("Detecting connected Ledger device...")
    devices = enumerate_devices()
    
    if not devices:
        print_error("No Ledger device detected.")
        print("Please ensure:")
        print("  1. The device is plugged in via USB.")
        print("  2. The device is unlocked (PIN entered).")
        print("  3. The Ledger dashboard is open (no app running).")
        sys.exit(1)
        
    if len(devices) > 1:
        print_warning(f"Multiple Ledger devices ({len(devices)}) detected. Using the first one.")
        
    device = devices[0]
    
    vid = getattr(device, 'vendor_id', None)
    pid = getattr(device, 'product_id', None)
    
    if vid is not None and vid != LEDGER_VENDOR_ID:
        print_error(f"Device found, but Vendor ID (0x{vid:04x}) is not Ledger (0x{LEDGER_VENDOR_ID:04x}).")
        sys.exit(1)
        
    if pid is None:
        print_error("Could not determine the Product ID of the connected device.")
        sys.exit(1)
        
    model_by_high_byte = {0x40: "nanox", 0x50: "nanosplus", 0x60: "stax", 0x70: "flex"}
    high_byte = pid >> 8
    slug = model_by_high_byte.get(high_byte)
    
    if not slug:
        print_error(f"Unsupported or unrecognized Ledger device (Product ID: 0x{pid:04x}).")
        print("Supported models: Nano S Plus, Nano X, Stax, Flex.")
        sys.exit(1)
        
    print_success(f"Detected Ledger model: {slug} (Product ID: 0x{pid:04x})")
    return slug

def get_release_data(tag):
    if tag:
        url = f"https://api.github.com/repos/tari-project/tari/releases/tags/{tag}"
    else:
        # Fetch all releases to allow picking the absolute latest even if it's a pre-release
        url = "https://api.github.com/repos/tari-project/tari/releases?per_page=1"
        
    try:
        response = requests.get(url, timeout=15)
        response.raise_for_status()
        data = response.json()
        if not tag and isinstance(data, list) and len(data) > 0:
            return data[0]
        elif not tag:
            print_error("No releases found.")
            sys.exit(1)
        return data
    except Exception as e:
        print_error(f"Failed to fetch release info from GitHub: {e}")
        sys.exit(1)

def download_and_extract(slug, release_data, temp_dir):
    print_header("Fetching Minotari release...")
    print(f"Release version: {release_data.get('tag_name', 'Unknown')}")
    
    target_suffix = f"-{slug}.zip"
    asset_url = None
    for asset in release_data.get("assets", []):
        if asset.get("name", "").endswith(target_suffix) and "minotari_ledger_wallet" in asset.get("name", ""):
            asset_url = asset["browser_download_url"]
            break
            
    if not asset_url:
        print_error(f"Could not find firmware for '{slug}' in the selected release.")
        sys.exit(1)
        
    print(f"Downloading: {asset_url}")
    zip_path = os.path.join(temp_dir, "firmware.zip")
    
    try:
        with requests.get(asset_url, stream=True, timeout=30) as r:
            r.raise_for_status()
            with open(zip_path, 'wb') as f:
                for chunk in r.iter_content(chunk_size=8192):
                    f.write(chunk)
                    
        print(f"Extracting zip archive...")
        with zipfile.ZipFile(zip_path, 'r') as zip_ref:
            zip_ref.extractall(temp_dir)
            
        apdu_path = None
        target_filename = "minotari_ledger_wallet.apdu"
        for root, _, files in os.walk(temp_dir):
            if target_filename in files:
                apdu_path = os.path.join(root, target_filename)
                break
                
        if not apdu_path:
            print_error(f"Failed to find '{target_filename}' inside the downloaded archive.")
            sys.exit(1)
            
        return apdu_path
    except Exception as e:
        print_error(f"Failed to download or extract firmware: {e}")
        sys.exit(1)

def install_app(slug, apdu_path):
    print_header("Installing Minotari app to Ledger...")
    print("If your device prompts you to allow an unsafe manager, please confirm it.")
    
    target_ids = {
        "nanox": "0x33000004",
        "nanosplus": "0x33100004",
        "stax": "0x33200004",
        "flex": "0x33300004"
    }
    
    if slug not in target_ids:
        print_error(f"No targetId mapping for model '{slug}'.")
        sys.exit(1)
    
    try:
        # Remove any previous install (best effort) so the fresh load does not clash.
        print("Removing previous installation (if any)...")
        subprocess.run(["ledgerctl", "delete", "MinoTari Wallet"], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        
        print(f"Flashing new firmware to {slug}...")
        # Replay the .apdu install script over a secure channel
        subprocess.run([
            sys.executable, "-m", "ledgerblue.runScript", 
            "--targetId", target_ids[slug], 
            "--fileName", apdu_path, 
            "--apdu", "--scp"
        ], check=True)
        print_success("Minotari app installed successfully!")
    except subprocess.CalledProcessError as e:
        print_error(f"ledgerblue installation failed with exit code {e.returncode}.")
        print("Ensure the device is unlocked, on the dashboard, and Developer Mode is enabled.")
        sys.exit(1)
    except FileNotFoundError:
        print_error("Command not found. Ensure this script is running via 'uv run'.")
        sys.exit(1)

def main():
    print(r"""
    __  __ _             _             _ 
   |  \/  (_)_ __   ___ | |_ __ _ _ __(_)
   | |\/| | | '_ \ / _ \| __/ _` | '__| |
   | |  | | | | | | (_) | || (_| | |  | |
   |_|  |_|_|_| |_|\___/ \__\__,_|_|  |_|
   Ledger Installer                     
    """)
    
    parser = argparse.ArgumentParser(description="Install Minotari app to your Ledger device.")
    parser.add_argument("-t", "--tag", help="Specific release tag to install (e.g. v5.4.0-pre.3). Defaults to the absolute latest (including pre-releases).")
    args, unknown = parser.parse_known_args()
    
    try:
        slug = detect_device()
        release_data = get_release_data(args.tag)
        
        with tempfile.TemporaryDirectory(prefix="minotari_ledger_") as temp_dir:
            apdu_path = download_and_extract(slug, release_data, temp_dir)
            install_app(slug, apdu_path)
            
    except KeyboardInterrupt:
        print_error("\nInstallation aborted by user.")
        sys.exit(1)

if __name__ == "__main__":
    main()
