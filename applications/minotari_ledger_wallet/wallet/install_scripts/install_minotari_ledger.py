#!/usr/bin/env python3
"""
Unified Minotari Ledger Wallet Installer

Automatically detects the connected Ledger model (Nano S Plus, Nano X, Stax,
Flex) and installs the correct Minotari app firmware - no user input needed.

The original Nano S is NOT supported by Minotari and is intentionally omitted.

Supports: macOS, Windows, Linux
Requires: Python 3.8+, pip, a Ledger device connected via USB
"""

import sys
import os
import subprocess
import platform
import json
import tempfile
import zipfile
import shutil
import urllib.request
import urllib.error
from pathlib import Path

# ---------------------------------------------------------------------------
# Ledger USB identifiers
# ---------------------------------------------------------------------------

LEDGER_VENDOR_ID = 0x2C97

# Product ID -> model slug mapping.
# Slugs match the release asset names: minotari_ledger_wallet-<slug>...zip
# and the app manifest: app_<slug>.json
PID_TO_MODEL = {
    # Nano S Plus
    0x0004: "nanosplus",
    0x4000: "nanosplus",  # alternate transport PID
    # Nano X
    0x0005: "nanox",
    0x5000: "nanox",  # alternate transport PID
    # Stax
    0x0006: "stax",
    0x6000: "stax",  # alternate transport PID
    # Flex
    0x0007: "flex",
    0x7000: "flex",  # alternate transport PID
}

MODEL_NAMES = {
    "nanosplus": "Nano S Plus",
    "nanox": "Nano X",
    "stax": "Stax",
    "flex": "Flex",
}

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

GITHUB_API = "https://api.github.com/repos/tari-project/tari/releases/latest"
APP_JSON_PATTERN = "app_{slug}.json"
ASSET_PATTERN = "minotari_ledger_wallet-{slug}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def warn(msg):
    print(f"\u26a0\ufe0f  {msg}")


def info(msg):
    print(f"\u2139\ufe0f  {msg}")


def success(msg):
    print(f"\u2705  {msg}")


def step(msg):
    print(f"\n\u{1f4cd} {msg}")


def die(msg):
    print(f"\u274c  {msg}")
    sys.exit(1)


def require_python_version():
    """Ensure Python >= 3.8."""
    if sys.version_info < (3, 8):
        die("Python 3.8 or later is required. Found: " + sys.version.split()[0])


# ---------------------------------------------------------------------------
# Ledger device detection
# ---------------------------------------------------------------------------


def _detect_via_hid():
    """
    Attempt USB HID detection using the 'hid' package.
    Returns the model slug string or None.
    """
    try:
        import hid
    except ImportError:
        return None

    for dev in hid.enumerate():
        vid = dev.get("vendor_id")
        pid = dev.get("product_id")
        if vid == LEDGER_VENDOR_ID:
            slug = PID_TO_MODEL.get(pid)
            if slug:
                return slug
    return None


def _detect_via_ledgerctl(ledgerctl_path):
    """
    Fallback: parse `ledgerctl info` output for the model identifier.
    Returns the model slug string or None.
    """
    try:
        result = subprocess.run(
            [str(ledgerctl_path), "info"],
            capture_output=True,
            text=True,
            timeout=15,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return None

    if result.returncode != 0:
        return None

    output = (result.stdout + result.stderr).lower()

    # ledgerctl info may output lines like:
    #   "Model: Nano X" / "Target ID: nanox" / "model_id": "nanox"
    for slug, name in MODEL_NAMES.items():
        name_lc = name.lower()
        if slug in output or name_lc in output:
            return slug

    # Also check for raw PID mentions (less common)
    if "nanos plus" in output or "nano s plus" in output:
        return "nanosplus"

    return None


def detect_ledger_model(ledgerctl_path=None):
    """
    Detect the connected Ledger model.
    Tries USB HID first, then falls back to ledgerctl info.
    Returns the model slug string, or exits on failure.
    """
    step("Detecting connected Ledger device...")

    # Try USB HID detection
    slug = _detect_via_hid()
    if slug:
        name = MODEL_NAMES.get(slug, slug)
        info(f"Detected Ledger {name} via USB HID")
        return slug

    info("USB HID detection unavailable (install 'hid' package for direct detection)")

    # Fallback to ledgerctl
    if ledgerctl_path and Path(ledgerctl_path).exists():
        info("Trying ledgerctl info fallback...")
        slug = _detect_via_ledgerctl(ledgerctl_path)
        if slug:
            name = MODEL_NAMES.get(slug, slug)
            info(f"Detected Ledger {name} via ledgerctl")
            return slug

    die(
        "No supported Ledger device detected.\n"
        "Make sure your Ledger is:\n"
        "  \u2022 Connected via USB\n"
        "  \u2022 Unlocked\n"
        "  \u2022 Developer Mode enabled\n"
        "  \u2022 Not running another app\n\n"
        "Supported models: " + ", ".join(MODEL_NAMES.values())
    )


# ---------------------------------------------------------------------------
# Virtual environment & dependency management
# ---------------------------------------------------------------------------


def ensure_venv_and_deps(project_dir):
    """
    Ensure the Python virtual environment exists and has the required packages.
    Returns the path to the venv's Python executable.
    """
    venv_dir = project_dir / ".venv"

    is_windows = platform.system() == "Windows"
    python_in_venv = (
        venv_dir / "Scripts" / "python.exe" if is_windows else venv_dir / "bin" / "python"
    )
    pip_in_venv = (
        venv_dir / "Scripts" / "pip.exe" if is_windows else venv_dir / "bin" / "pip"
    )
    ledgerctl_in_venv = (
        venv_dir / "Scripts" / "ledgerctl.exe"
        if is_windows
        else venv_dir / "bin" / "ledgerctl"
    )

    if venv_dir.exists() and ledgerctl_in_venv.exists():
        info("Virtual environment already set up")
        return str(python_in_venv), str(ledgerctl_in_venv)

    step("Setting up Python virtual environment...")
    venv_dir.mkdir(parents=True, exist_ok=True)

    result = subprocess.run(
        [sys.executable, "-m", "venv", str(venv_dir)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        die(f"Failed to create virtual environment:\n{result.stderr}")

    success(f"Virtual environment created at {venv_dir}")

    step("Installing dependencies...")
    for pkg in ["protobuf", "setuptools", "ecdsa", "ledgerwallet", "ledgerctl", "hid"]:
        result = subprocess.run(
            [str(pip_in_venv), "install", "--quiet", pkg],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            warn(f"Failed to install {pkg}: {result.stderr.strip()}")
            # Non-fatal: hid is optional, others are needed
            if pkg not in ("hid",):
                die(f"Required dependency {pkg} failed to install")

    success("Dependencies installed")
    return str(python_in_venv), str(ledgerctl_in_venv)


# ---------------------------------------------------------------------------
# Release download
# ---------------------------------------------------------------------------


def find_asset_url(model_slug):
    """
    Query the GitHub API for the latest release and find the matching asset.
    Returns (download_url, asset_name) or exits on failure.
    """
    step(f"Fetching latest Minotari Ledger release for {MODEL_NAMES[model_slug]}...")

    headers = {"User-Agent": "minotari-ledger-installer", "Accept": "application/vnd.github.v3+json"}

    # Check for GITHUB_TOKEN in environment for higher rate limits
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = "token " + token

    try:
        req = urllib.request.Request(GITHUB_API, headers=headers)
        with urllib.request.urlopen(req, timeout=30) as resp:
            release = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        die(f"GitHub API error ({e.code}): {e.reason}")
    except Exception as e:
        die(f"Failed to fetch release info: {e}")

    tag = release.get("tag_name", "unknown")
    info(f"Latest release: {tag}")

    # Find the matching asset
    pattern = ASSET_PATTERN.format(slug=model_slug)
    for asset in release.get("assets", []):
        name = asset["name"]
        if pattern in name.lower() and name.endswith(".zip"):
            info(f"Found asset: {name}")
            return asset["browser_download_url"], name

    # List available assets for debugging
    available = [a["name"] for a in release.get("assets", []) if "ledger" in a["name"].lower()]
    die(
        f"No matching release asset found for model '{model_slug}'.\n"
        f"Available ledger assets: {available}"
    )


def download_and_extract(download_url, asset_name, download_dir):
    """
    Download the release zip and extract it.
    Returns the path to the extracted directory.
    """
    step(f"Downloading {asset_name}...")

    zip_path = download_dir / asset_name
    try:
        urllib.request.urlretrieve(download_url, zip_path)
    except Exception as e:
        die(f"Download failed: {e}")

    success(f"Downloaded to {zip_path}")

    step("Extracting archive...")
    extract_dir = download_dir / asset_name.replace(".zip", "")
    extract_dir.mkdir(parents=True, exist_ok=True)

    try:
        with zipfile.ZipFile(zip_path, "r") as zf:
            zf.extractall(extract_dir)
    except zipfile.BadZipFile:
        die(f"Downloaded file is not a valid zip archive")

    success(f"Extracted to {extract_dir}")
    return extract_dir


# ---------------------------------------------------------------------------
# Installation
# ---------------------------------------------------------------------------


def find_app_json(extract_dir, model_slug):
    """Find the app manifest JSON in the extracted directory."""
    pattern = APP_JSON_PATTERN.format(slug=model_slug)

    for root, _dirs, files in os.walk(extract_dir):
        for fname in files:
            if fname == pattern or fname == f"app_{model_slug}.json":
                return Path(root) / fname

    # Fallback: any app_*.json
    for root, _dirs, files in os.walk(extract_dir):
        for fname in files:
            if fname.startswith("app_") and fname.endswith(".json"):
                return Path(root) / fname

    die(f"App manifest ({pattern}) not found in extracted archive")


def install_app(ledgerctl_path, app_json_path):
    """
    Run ledgerctl install with the app manifest.
    Returns True on success, False on failure.
    """
    step("Installing Minotari app onto Ledger device...")
    warn("Please confirm the installation on your Ledger device.")

    result = subprocess.run(
        [str(ledgerctl_path), "install", str(app_json_path)],
        capture_output=True,
        text=True,
        timeout=120,
        cwd=str(app_json_path.parent),
    )

    if result.stdout:
        print(result.stdout)

    if result.returncode == 0:
        success("Minotari Ledger Wallet installed successfully!")
        return True
    else:
        if result.stderr:
            print(result.stderr)
        die("Installation failed. Check that your Ledger is connected, unlocked, and in Developer Mode.")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main():
    print("\u{1f680} Minotari Ledger Wallet Installer")
    print("=" * 50)

    require_python_version()

    # Project directory (next to this script, or current dir)
    script_dir = Path(__file__).parent.resolve()
    project_dir = script_dir
    download_dir = project_dir / "downloads"

    # Step 1: Set up venv + deps
    python_path, ledgerctl_path = ensure_venv_and_deps(project_dir)

    # Step 2: Detect Ledger model
    # We need to run detection in a way that can use the 'hid' package from venv
    # First try without venv (system hid), then with venv
    slug = _detect_via_hid()
    if not slug:
        # Try importing hid from the venv
        is_windows = platform.system() == "Windows"
        site_packages = (
            project_dir / ".venv" / "Lib" / "site-packages"
            if is_windows
            else project_dir / ".venv" / "lib" / f"python{sys.version_info.major}.{sys.version_info.minor}" / "site-packages"
        )
        if site_packages.exists():
            sys.path.insert(0, str(site_packages))
            slug = _detect_via_hid()

    if not slug:
        slug = detect_ledger_model(ledgerctl_path)

    name = MODEL_NAMES.get(slug, slug)
    info(f"Target device: Ledger {name}")

    # Step 3: Download release
    download_url, asset_name = find_asset_url(slug)
    download_dir.mkdir(parents=True, exist_ok=True)
    extract_dir = download_and_extract(download_url, asset_name, download_dir)

    # Step 4: Install
    app_json = find_app_json(extract_dir, slug)
    info(f"App manifest: {app_json}")
    install_app(ledgerctl_path, app_json)

    # Cleanup
    step("Cleaning up...")
    try:
        shutil.rmtree(extract_dir, ignore_errors=True)
        (download_dir / asset_name).unlink(missing_ok=True)
    except OSError:
        pass

    print()
    success("All done! You can now use the Minotari app on your Ledger device.")
    print()


if __name__ == "__main__":
    main()
