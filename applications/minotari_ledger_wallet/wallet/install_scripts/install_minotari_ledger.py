# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "ledgerwallet",
#     "ledgerctl",
#     "requests"
# ]
# ///

import sys
import os
import subprocess
import requests
import zipfile
import tempfile
import time
import shutil
from urllib.parse import urlparse
from ledgerwallet.transport import enumerate_devices

# ---------------------------------------------------------------------------
# Ledger USB Product IDs to Tari Slug Mapping
# ---------------------------------------------------------------------------
LEDGER_VENDOR_ID = 0x2C97

LEDGER_MODELS = {
    # Nano S Plus
    0x0004: "nanosplus",
    0x4000: "nanosplus",
    0x5011: "nanosplus",
    # Nano X
    0x0005: "nanox",
    0x4005: "nanox",
    0x5000: "nanox",
    0x5015: "nanox",
    # Stax
    0x0006: "stax",
    0x6000: "stax",
    0x6011: "stax",
    # Flex
    0x0007: "flex",
    0x7000: "flex",
    0x7011: "flex",
}

GITHUB_API_URL = "https://api.github.com/repos/tari-project/tari/releases/latest"

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
        
    device = devices[0]
    
    if device.vendor_id != LEDGER_VENDOR_ID:
        print_error(f"Device found, but Vendor ID (0x{device.vendor_id:04x}) is not Ledger (0x{LEDGER_VENDOR_ID:04x}).")
        sys.exit(1)
        
    slug = LEDGER_MODELS.get(device.product_id)
    if not slug:
        print_error(f"Unsupported or unrecognized Ledger device (Product ID: 0x{device.product_id:04x}).")
        print("Supported models: Nano S Plus, Nano X, Stax, Flex.")
        sys.exit(1)
        
    print_success(f"Detected Ledger model: {slug} (Product ID: 0x{device.product_id:04x})")
    return slug

def download_and_extract(slug):
    print_header("Fetching latest Minotari release...")
    try:
        response = requests.get(GITHUB_API_URL, timeout=15)
        response.raise_for_status()
        release_data = response.json()
    except Exception as e:
        print_error(f"Failed to fetch latest release from GitHub: {e}")
        sys.exit(1)

    print(f"Latest release: {release_data.get('tag_name', 'Unknown')}")
    
    target_suffix = f"-{slug}.zip"
    asset_url = None
    for asset in release_data.get("assets", []):
        if asset.get("name", "").endswith(target_suffix) and "minotari_ledger_wallet" in asset.get("name", ""):
            asset_url = asset["browser_download_url"]
            break
            
    if not asset_url:
        print_error(f"Could not find firmware for '{slug}' in the latest release.")
        sys.exit(1)
        
    print(f"Downloading: {asset_url}")
    
    temp_dir = tempfile.mkdtemp(prefix="minotari_ledger_")
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
            
        app_json_path = None
        for root, _, files in os.walk(temp_dir):
            for file in files:
                if file.endswith(".json") and "app_" in file:
                    app_json_path = os.path.join(root, file)
                    break
            if app_json_path:
                break
                
        if not app_json_path:
            print_error("Failed to find 'app.json' inside the downloaded archive.")
            sys.exit(1)
            
        return app_json_path
    except Exception as e:
        print_error(f"Failed to download or extract firmware: {e}")
        sys.exit(1)
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

def install_app(app_json_path):
    print_header("Installing Minotari app to Ledger...")
    print("If your device prompts you to allow an unsafe manager, please confirm it.")
    time.sleep(1)
    
    try:
        subprocess.run(["ledgerctl", "install", app_json_path], check=True)
        print_success("Minotari app installed successfully!")
    except subprocess.CalledProcessError as e:
        print_error(f"ledgerctl failed with exit code {e.returncode}.")
        print("Ensure the device is unlocked and on the dashboard.")
        sys.exit(1)
    except FileNotFoundError:
        print_error("ledgerctl command not found. Ensure this script is running via 'uv run'.")
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
    try:
        slug = detect_device()
        app_json = download_and_extract(slug)
        install_app(app_json)
    except KeyboardInterrupt:
        print_error("\nInstallation aborted by user.")
        sys.exit(1)

if __name__ == "__main__":
    main()
