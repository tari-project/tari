#!/usr/bin/env python3
"""
Unified Minotari Ledger Wallet Installer
Auto-detects Ledger model and installs the correct app across macOS, Windows, and Linux.
"""

import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import zipfile
from typing import Optional


class Colors:
    CYAN = "\033[96m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    RESET = "\033[0m"


def print_header(msg: str) -> None:
    print(f"{Colors.CYAN}{msg}{Colors.RESET}")


def print_success(msg: str) -> None:
    print(f"{Colors.GREEN}OK: {msg}{Colors.RESET}")


def print_info(msg: str) -> None:
    print(f"{Colors.CYAN}INFO: {msg}{Colors.RESET}")


def print_warn(msg: str) -> None:
    print(f"{Colors.YELLOW}WARN: {msg}{Colors.RESET}")


def print_error(msg: str) -> None:
    print(f"{Colors.RED}ERROR: {msg}{Colors.RESET}", file=sys.stderr)
    sys.exit(1)


def check_python_version() -> None:
    if sys.version_info < (3, 7):
        print_error(
            f"Python 3.7+ required (you have {sys.version_info.major}.{sys.version_info.minor})"
        )


def check_command(cmd: str) -> bool:
    return shutil.which(cmd) is not None


def install_dependencies() -> None:
    required_packages = ["protobuf", "setuptools", "ecdsa", "ledgerwallet", "ledgerctl"]

    print_info("Installing Python dependencies...")
    for package in required_packages:
        try:
            __import__(package)
            print_success(f"{package} already installed")
        except ImportError:
            print_info(f"Installing {package}...")
            subprocess.check_call([sys.executable, "-m", "pip", "install", package])
            print_success(f"{package} installed")


def detect_ledger_model() -> Optional[str]:
    """
    Detect connected Ledger device and return its model.
    Returns: 'flex', 'nanos', 'nanosplus', 'nanox', 'stax', or None if no device found.
    """
    print_info("Detecting Ledger device...")

    try:
        result = subprocess.run(
            ["ledgerctl", "list"],
            capture_output=True,
            text=True,
            timeout=10,
        )

        if result.returncode != 0:
            return None

        output = result.stdout.lower()

        model_keywords = [
            ("flex", "flex"),
            ("nano s plus", "nanosplus"),
            ("nano s", "nanos"),
            ("nano x", "nanox"),
            ("stax", "stax"),
            ("nanosplus", "nanosplus"),
            ("nanos", "nanos"),
            ("nanox", "nanox"),
        ]

        for keyword, model in model_keywords:
            if keyword in output:
                return model

        print_warn("Ledger device detected but model not recognized")
        return None

    except FileNotFoundError:
        print_error("ledgerctl not found. Install it via: pip install ledgerctl")
    except subprocess.TimeoutExpired:
        print_error("Device detection timed out. Ensure device is connected and unlocked.")
    except Exception as exc:
        print_error(f"Error detecting Ledger: {exc}")


def get_ledger_model_from_user() -> str:
    models = ["flex", "nanos", "nanosplus", "nanox", "stax"]

    print("\nCould not auto-detect Ledger model.")
    print("Please select your Ledger model:")
    for i, model in enumerate(models, 1):
        print(f"  {i}. {model}")

    while True:
        try:
            choice = input("Enter model number (1-5): ").strip()
            idx = int(choice) - 1
            if 0 <= idx < len(models):
                return models[idx]
        except (ValueError, IndexError):
            pass
        print("Invalid selection. Please try again.")


def download_release(model: str, dest_dir: str) -> str:
    print_info(f"Fetching latest Minotari Ledger release for {model}...")

    try:
        url = "https://api.github.com/repos/tari-project/tari/releases/latest"
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "minotari-ledger-installer"},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            release_data = json.loads(response.read().decode())

        asset = None
        pattern = f"minotari_ledger_wallet-{model}"
        for item in release_data.get("assets", []):
            if pattern in item["name"] and item["name"].endswith(".zip"):
                asset = item
                break

        if not asset:
            print_error(f"No release found for model '{model}'")

        asset_url = asset["browser_download_url"]
        asset_name = asset["name"]

        print_info(f"Downloading {asset_name}...")
        dest_path = os.path.join(dest_dir, asset_name)

        def progress_hook(block_num: int, block_size: int, total_size: int) -> None:
            downloaded = block_num * block_size
            if total_size > 0:
                percent = min(100, (downloaded * 100) // total_size)
                print(f"\r  Progress: {percent}%", end="", flush=True)

        urllib.request.urlretrieve(asset_url, dest_path, progress_hook)
        print()
        print_success(f"Downloaded to {dest_path}")

        return dest_path

    except urllib.error.URLError as exc:
        print_error(f"Download failed: {exc}")
    except json.JSONDecodeError:
        print_error("Failed to parse GitHub release data")
    except Exception as exc:
        print_error(f"Unexpected error during download: {exc}")


def extract_release(zip_path: str, extract_dir: str) -> str:
    print_info(f"Extracting {os.path.basename(zip_path)}...")

    try:
        with zipfile.ZipFile(zip_path, "r") as zip_ref:
            zip_ref.extractall(extract_dir)

        for root, _, files in os.walk(extract_dir):
            for filename in files:
                if filename.startswith("app_") and filename.endswith(".json"):
                    app_json = os.path.join(root, filename)
                    print_success(f"Found app manifest: {os.path.basename(app_json)}")
                    return app_json

        print_error("No app manifest (app_*.json) found in release")

    except zipfile.BadZipFile:
        print_error("Downloaded file is not a valid zip archive")
    except Exception as exc:
        print_error(f"Extraction failed: {exc}")


def install_to_ledger(app_json_path: str) -> None:
    print("\n" + "=" * 60)
    print_warn("Prepare your Ledger device")
    print("=" * 60)
    print("Ensure your Ledger device has:")
    print("  - Connected via USB")
    print("  - Unlocked")
    print("  - Developer Mode enabled (if required for your model)")
    print("=" * 60 + "\n")

    input("Press Enter when ready to install...")

    print_info(f"Installing app from {os.path.basename(app_json_path)}...")

    try:
        result = subprocess.run(["ledgerctl", "install", app_json_path], timeout=120)
        if result.returncode != 0:
            print_error("Installation failed. Check device status and try again.")
        print_success("Minotari Ledger Wallet installed successfully.")

    except FileNotFoundError:
        print_error("ledgerctl not found")
    except subprocess.TimeoutExpired:
        print_error("Installation timed out")
    except Exception as exc:
        print_error(f"Installation failed: {exc}")


def setup_environment() -> None:
    check_python_version()
    print_info(f"Running on {platform.system()} {platform.release()}")

    if not check_command("pip") and not check_command("pip3"):
        print_error("pip not found. Install Python with pip enabled.")

    install_dependencies()


def main() -> None:
    print_header("Minotari Ledger Wallet Unified Installer")
    setup_environment()

    with tempfile.TemporaryDirectory() as temp_dir:
        model = detect_ledger_model()
        if not model:
            model = get_ledger_model_from_user()

        print_success(f"Using Ledger model: {model}")

        zip_path = download_release(model, temp_dir)
        extract_dir = os.path.join(temp_dir, "extracted")
        os.makedirs(extract_dir, exist_ok=True)
        app_json_path = extract_release(zip_path, extract_dir)
        install_to_ledger(app_json_path)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print_error("Installation cancelled by user")
    except Exception as exc:
        print_error(f"Unexpected error: {exc}")
