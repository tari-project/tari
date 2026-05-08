#!/usr/bin/env python3
"""
Unified Minotari Ledger Wallet Installer

Automatically detects the connected Ledger model (Nano S Plus, Nano X, Stax,
Flex) and installs the correct Minotari app firmware — no user input needed.

The original Nano S is **not** supported by Minotari and is intentionally
omitted here.

Supports: macOS, Windows, Linux
Requires: Python 3.8+, pip
"""

import sys
import os
import subprocess
import platform
import json
import tempfile
import urllib.request
import urllib.error
import zipfile

# ---------------------------------------------------------------------------
# Ledger USB identifiers
# ---------------------------------------------------------------------------

# All Ledger devices share vendor ID 0x2c97
LEDGER_VENDOR_ID = 0x2C97

# Slugs match the asset names in `ledger_app.toml` and the published release
# bundles (`minotari_ledger_wallet-<slug>...zip`, `app_<slug>.json`). In
# particular Nano S Plus is `nanos+`, NOT `nanosplus`.
#
# Some firmware versions expose different product IDs per transport;
# we keep a broader map covering all known values.
LEDGER_PRODUCT_IDS: dict = {
    # Nano S Plus
    0x0004: "nanos+",
    0x4000: "nanos+",
    # Nano X
    0x0005: "nanox",
    0x4005: "nanox",
    0x5000: "nanox",
    # Stax
    0x0006: "stax",
    0x6000: "stax",
    # Flex
    0x0007: "flex",
    0x7000: "flex",
}

GITHUB_RELEASES_URL = (
    "https://api.github.com/repos/tari-project/tari/releases/latest"
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def info(msg):
    print(f"  {msg}")


def success(msg):
    print(f"\u2705 {msg}")


def error(msg):
    print(f"\u274c {msg}", file=sys.stderr)


def warn(msg):
    print(f"\u26a0\ufe0f  {msg}")


def run(cmd, **kwargs):
    return subprocess.run(cmd, check=True, **kwargs)


# ---------------------------------------------------------------------------
# Step 1 – install Python dependencies
# ---------------------------------------------------------------------------

def ensure_dependencies():
    print("\n\U0001f4e6 Installing Python dependencies...")
    # NOTE: ``ledgerctl`` is *not* a standalone PyPI package — the
    # ``ledgerctl`` command is provided by ``ledgerwallet``. Likewise
    # ``hid`` is pulled in transitively (as ``hidapi``) by ``ledgerwallet``,
    # so we don't request it directly: doing so used to install a
    # different, unrelated ``hid`` package and break import order.
    packages = ["ledgerwallet", "protobuf", "ecdsa"]
    run([sys.executable, "-m", "pip", "install", "--upgrade", "pip"], capture_output=True)
    run([sys.executable, "-m", "pip", "install"] + packages)
    success("Dependencies installed")


# ---------------------------------------------------------------------------
# Step 2 – detect Ledger model
# ---------------------------------------------------------------------------

def detect_ledger_model():
    """Return the model slug (e.g. 'flex') or raise RuntimeError."""

    print("\n\U0001f50d Detecting connected Ledger device...")

    # Try hid library first (most reliable cross-platform)
    try:
        import hid  # type: ignore
        devices = hid.enumerate(LEDGER_VENDOR_ID, 0)
        for dev in devices:
            pid = dev.get("product_id", 0)
            model = LEDGER_PRODUCT_IDS.get(pid)
            if model:
                success(f"Detected Ledger {model.title()} (PID=0x{pid:04x})")
                return model
    except ImportError:
        warn("hid module not installed — falling back to ledgerctl detection.")
    except Exception as exc:
        warn(f"hid enumeration failed ({exc}) — falling back to ledgerctl.")

    # Fallback: ask ledgerctl (via ledgerwallet) for device info.
    #
    # Match against full model names rather than slug substrings \u2014 the raw
    # output is something like "Connected to Ledger Nano S Plus", and a
    # naive substring check on "nanos" would mis-classify a Nano S Plus
    # as a Nano S. Order doesn't matter once we use full names.
    detection_map = {
        "nano s plus": "nanos+",
        "nano x": "nanox",
        "flex": "flex",
        "stax": "stax",
    }
    try:
        result = subprocess.run(
            [sys.executable, "-m", "ledgerwallet", "info"],
            capture_output=True, text=True, timeout=10
        )
        output = (result.stdout + result.stderr).lower()
        for pattern, slug in detection_map.items():
            if pattern in output:
                success(f"Detected Ledger {pattern} via ledgerctl (slug={slug})")
                return slug
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    raise RuntimeError(
        "No supported Ledger device detected.\n"
        "Minotari supports Nano S Plus, Nano X, Stax, and Flex (the original "
        "Nano S is not supported).\n"
        "Make sure the device is:\n"
        "  \u2022 Connected via USB\n"
        "  \u2022 Unlocked (PIN entered)\n"
        "  \u2022 Developer Mode enabled (Settings \u2192 Developer Mode)"
    )


# ---------------------------------------------------------------------------
# Step 3 – fetch latest release asset URL
# ---------------------------------------------------------------------------

def fetch_asset_url(model):
    """Return (download_url, filename) for the given model."""

    print(f"\n\U0001f310 Fetching latest Minotari Ledger release for '{model}'...")

    try:
        req = urllib.request.Request(
            GITHUB_RELEASES_URL,
            headers={"User-Agent": "minotari-ledger-installer/1.0"}
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            release = json.loads(resp.read())
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Failed to fetch release info: {exc}") from exc

    pattern = f"minotari_ledger_wallet-{model}"
    candidates = [
        a for a in release.get("assets", [])
        if pattern in a["name"].lower() and a["name"].endswith(".zip")
    ]

    if not candidates:
        available = [a["name"] for a in release.get("assets", [])]
        raise RuntimeError(
            f"No release asset found for model '{model}'.\n"
            f"Available assets: {available}"
        )

    asset = candidates[0]
    info(f"Found: {asset['name']}")
    return asset["browser_download_url"], asset["name"]


# ---------------------------------------------------------------------------
# Step 4 – download + extract
# ---------------------------------------------------------------------------

def download_and_extract(url, filename, dest_dir):
    """Download zip, extract, return path to extracted directory."""

    zip_path = os.path.join(dest_dir, filename)
    print(f"\n\u2b07\ufe0f  Downloading {filename}...")

    def progress(count, block_size, total_size):
        if total_size > 0:
            pct = min(100, count * block_size * 100 // total_size)
            print(f"\r   {pct}%", end="", flush=True)

    urllib.request.urlretrieve(url, zip_path, reporthook=progress)
    print()  # newline after progress

    print(f"\U0001f4e6 Extracting {filename}...")
    with zipfile.ZipFile(zip_path, "r") as zf:
        zf.extractall(dest_dir)

    success("Download and extraction complete")
    return dest_dir


# ---------------------------------------------------------------------------
# Step 5 – find and install app JSON
# ---------------------------------------------------------------------------

def find_app_json(model, search_dir):
    """Find the app_<model>.json manifest."""

    candidates = []
    for root, _dirs, files in os.walk(search_dir):
        for fname in files:
            if fname == f"app_{model}.json":
                candidates.append(os.path.join(root, fname))

    if not candidates:
        # Fallback: any app*.json
        for root, _dirs, files in os.walk(search_dir):
            for fname in files:
                if fname.startswith("app") and fname.endswith(".json"):
                    candidates.append(os.path.join(root, fname))

    if not candidates:
        raise RuntimeError(
            f"Could not find app_{model}.json after extraction in {search_dir}"
        )

    return candidates[0]


def install_app(app_json):
    print(f"\n\U0001f510 Installing app from: {app_json}")
    print("\U0001f449 Make sure:")
    print("   \u2022 Ledger is connected via USB")
    print("   \u2022 Device is unlocked")
    print("   \u2022 Developer Mode is enabled\n")
    # Use ``sys.executable -m ledgerwallet`` rather than a bare
    # ``ledgerctl`` invocation: the latter relies on the active venv's
    # console-scripts being on PATH, which is unreliable when this script
    # is run from a non-interactive subprocess (CI, double-click launcher,
    # etc.). Going through the interpreter pins the same Python install
    # we used for ``ensure_dependencies``.
    run([sys.executable, "-m", "ledgerwallet", "install", app_json])


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("=" * 60)
    print(" \U0001f680 Minotari Ledger Wallet \u2014 Unified Installer")
    print(f"    Platform: {platform.system()} {platform.machine()}")
    print("=" * 60)

    # Install deps first so hid is available for detection
    ensure_dependencies()

    # Detect model
    try:
        model = detect_ledger_model()
    except RuntimeError as exc:
        error(str(exc))
        sys.exit(1)

    # Fetch asset
    try:
        url, filename = fetch_asset_url(model)
    except RuntimeError as exc:
        error(str(exc))
        sys.exit(1)

    # Download + extract into a temp dir. Using ``TemporaryDirectory`` as a
    # context manager guarantees cleanup even if the script is interrupted
    # or an unexpected (non-RuntimeError) exception bubbles up — the manual
    # mkdtemp / shutil.rmtree pair previously leaked the directory in those
    # cases.
    with tempfile.TemporaryDirectory(prefix="minotari_ledger_") as tmp_dir:
        try:
            download_and_extract(url, filename, tmp_dir)
            app_json = find_app_json(model, tmp_dir)
            install_app(app_json)
        except RuntimeError as exc:
            error(str(exc))
            sys.exit(1)

    print()
    success("Minotari Ledger Wallet installed successfully!")
    print()


if __name__ == "__main__":
    main()
