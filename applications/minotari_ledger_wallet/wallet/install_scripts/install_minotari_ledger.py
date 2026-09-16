#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = [
#     "ledgerwallet",
#     "ledgerblue",
# ]
# ///
"""
Unified installer for the Minotari Ledger Wallet application.

The installer detects the connected Ledger model, downloads the matching Tari
release artifact, verifies it, and installs it. It supports Nano S Plus, Nano X,
Stax, and Flex. The original Nano S is intentionally unsupported.

The Ledger tooling this installer needs is declared inline (PEP 723) so the
`install_minotari_ledger.sh` / `install_minotari_ledger.ps1` launchers can run it
with `uv run`, which provisions an isolated Python plus dependencies on demand.
No system Python or pip setup is required.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable, Optional, Sequence

GITHUB_REPO = "tari-project/tari"
GITHUB_API = f"https://api.github.com/repos/{GITHUB_REPO}"
USER_AGENT = "minotari-ledger-installer/1.0"

# Name the app registers on the device (from wallet/Cargo.toml [package.metadata.ledger]).
# Used to remove a previous install before flashing a fresh one.
LEDGER_APP_NAME = "MinoTari Wallet"


@dataclass(frozen=True)
class LedgerModel:
    slug: str
    display_name: str
    target_id: int


SUPPORTED_MODELS = {
    "nanosplus": LedgerModel("nanosplus", "Ledger Nano S Plus", 0x33100004),
    "nanox": LedgerModel("nanox", "Ledger Nano X", 0x33000004),
    "stax": LedgerModel("stax", "Ledger Stax", 0x33200004),
    "flex": LedgerModel("flex", "Ledger Flex", 0x33300004),
}

MODEL_BY_TARGET_ID = {model.target_id: model for model in SUPPORTED_MODELS.values()}

UNSUPPORTED_TARGET_IDS = {
    0x31100002: "Ledger Nano S",
    0x31100003: "Ledger Nano S",
    0x31100004: "Ledger Nano S",
}


@dataclass(frozen=True)
class ReleaseAsset:
    tag_name: str
    asset_name: str
    download_url: str
    checksum_url: str


class InstallerError(Exception):
    """Expected installer failure with a user-facing message."""


_ANSI = {
    "reset": "\033[0m",
    "cyan": "\033[1;36m",
    "green": "\033[1;32m",
    "yellow": "\033[1;33m",
    "red": "\033[1;31m",
}


def _enable_windows_ansi() -> None:
    """Best-effort enabling of ANSI escape processing on legacy Windows consoles."""
    if sys.platform != "win32":
        return
    try:
        import ctypes

        kernel32 = ctypes.windll.kernel32
        enable_virtual_terminal_processing = 0x0004
        for handle_id in (-11, -12):  # STD_OUTPUT_HANDLE, STD_ERROR_HANDLE
            handle = kernel32.GetStdHandle(handle_id)
            mode = ctypes.c_uint32()
            if kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
                kernel32.SetConsoleMode(handle, mode.value | enable_virtual_terminal_processing)
    except Exception:  # pragma: no cover - colour is a nicety, never fatal
        pass


def _color_enabled(stream) -> bool:
    if os.environ.get("NO_COLOR") is not None:
        return False
    if os.environ.get("TERM") == "dumb":
        return False
    return bool(getattr(stream, "isatty", lambda: False)())


def _style(text: str, color: str, stream) -> str:
    if not _color_enabled(stream):
        return text
    return f"{_ANSI[color]}{text}{_ANSI['reset']}"


def print_step(message: str) -> None:
    print(f"{_style('==>', 'cyan', sys.stdout)} {message}")


def print_info(message: str) -> None:
    print(f"    {message}")


def print_success(message: str) -> None:
    print(f"{_style('==>', 'green', sys.stdout)} {message}")


def print_warning(message: str) -> None:
    print(f"{_style('==> WARNING:', 'yellow', sys.stderr)} {message}", file=sys.stderr)


def print_error(message: str) -> None:
    print(f"{_style('Error:', 'red', sys.stderr)} {message}", file=sys.stderr)


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install the Minotari Ledger Wallet app for the connected Ledger device.",
    )
    parser.add_argument(
        "-t",
        "--tag",
        help="Install a specific Tari release tag, e.g. v5.4.0-pre.1.",
    )
    return parser.parse_args(argv)


def github_request(url: str) -> urllib.request.Request:
    return urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": USER_AGENT,
        },
    )


def fetch_json(url: str):
    try:
        with urllib.request.urlopen(github_request(url), timeout=30) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        raise InstallerError(f"GitHub request failed: HTTP {error.code} for {url}") from error
    except urllib.error.URLError as error:
        raise InstallerError(f"Network error while contacting GitHub: {error.reason}") from error
    except json.JSONDecodeError as error:
        raise InstallerError("GitHub returned invalid JSON") from error


def asset_matches_model(asset_name: str, model: str) -> bool:
    pattern = rf"^minotari_ledger_wallet-{re.escape(model)}-.+\.zip$"
    return re.match(pattern, asset_name) is not None


def find_asset_for_model(release: dict, model: str) -> Optional[ReleaseAsset]:
    assets = release.get("assets") or []
    checksum_assets = {asset.get("name", ""): asset for asset in assets}
    for zip_asset in assets:
        name = zip_asset.get("name", "")
        if not asset_matches_model(name, model):
            continue

        checksum_asset = checksum_assets.get(f"{name}.sha256")
        if checksum_asset is None:
            continue

        return ReleaseAsset(
            tag_name=release.get("tag_name", "unknown"),
            asset_name=name,
            download_url=zip_asset["browser_download_url"],
            checksum_url=checksum_asset["browser_download_url"],
        )

    return None


def select_asset_from_releases(releases: Iterable[dict], model: str) -> ReleaseAsset:
    for release in releases:
        if release.get("draft"):
            continue
        asset = find_asset_for_model(release, model)
        if asset is not None:
            return asset
    raise InstallerError(
        f"No non-draft Tari release contains a Minotari Ledger artifact for model '{model}'."
    )


def fetch_release_asset(model: str, tag: Optional[str]) -> ReleaseAsset:
    if tag:
        release = fetch_json(f"{GITHUB_API}/releases/tags/{urllib.parse.quote(tag)}")
        if release.get("draft"):
            raise InstallerError(f"Release {tag} is a draft and cannot be installed.")
        asset = find_asset_for_model(release, model)
        if asset is None:
            available = [
                asset.get("name", "")
                for asset in release.get("assets", [])
                if "minotari_ledger_wallet" in asset.get("name", "")
            ]
            raise InstallerError(
                f"Release {tag} has no verified Ledger artifact for model '{model}'. "
                f"Available Ledger assets: {available or 'none'}"
            )
        return asset

    for page in range(1, 4):
        releases = fetch_json(f"{GITHUB_API}/releases?per_page=30&page={page}")
        if not releases:
            break
        try:
            return select_asset_from_releases(releases, model)
        except InstallerError:
            continue

    raise InstallerError(
        f"No recent Tari release contains a verified Minotari Ledger artifact for model '{model}'."
    )


def download_file(url: str, destination: Path) -> None:
    try:
        with urllib.request.urlopen(github_request(url), timeout=120) as response:
            try:
                total = int(response.headers.get("Content-Length") or 0)
            except (TypeError, ValueError):
                total = 0
            downloaded = 0
            with destination.open("wb") as output:
                while True:
                    chunk = response.read(1024 * 64)
                    if not chunk:
                        break
                    output.write(chunk)
                    downloaded += len(chunk)
                    if total:
                        pct = min(100, downloaded * 100 // total)
                        print(f"\r    Downloaded {pct:3d}%", end="", flush=True)
            if total:
                print()
    except urllib.error.URLError as error:
        raise InstallerError(f"Download failed: {error.reason}") from error
    except OSError as error:
        raise InstallerError(f"Download failed: {error}") from error


def parse_sha256_file(text: str, expected_filename: str) -> str:
    digests = []
    found_named_digest = False
    for line in text.splitlines():
        parts = line.strip().split()
        if not parts:
            continue
        digest = parts[0].removeprefix("sha256:")
        if not re.fullmatch(r"[0-9a-fA-F]{64}", digest):
            continue
        filename = parts[-1].lstrip("*") if len(parts) > 1 else None
        if filename == expected_filename:
            return digest.lower()
        if filename is not None:
            found_named_digest = True
            continue
        digests.append(digest.lower())
    if len(digests) == 1 and not found_named_digest:
        return digests[0]
    if digests or found_named_digest:
        raise InstallerError(f"Checksum file did not contain a digest for {expected_filename}.")
    raise InstallerError("Checksum file did not contain a SHA256 digest.")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sha256(path: Path, expected_digest: str) -> None:
    try:
        actual = sha256_file(path)
    except OSError as error:
        raise InstallerError(f"Could not read downloaded archive {path.name}: {error}") from error
    if actual.lower() != expected_digest.lower():
        raise InstallerError(
            f"Checksum mismatch for {path.name}: expected {expected_digest}, got {actual}."
        )


def safe_extract(zip_path: Path, extract_dir: Path) -> None:
    root = extract_dir.resolve()
    try:
        with zipfile.ZipFile(zip_path, "r") as archive:
            for member in archive.infolist():
                name = member.filename
                normalized = name.replace("\\", "/")
                path = PurePosixPath(normalized)
                if (
                    not name
                    or "\0" in name
                    or ":" in name
                    or path.is_absolute()
                    or ".." in path.parts
                ):
                    raise InstallerError(f"Unsafe path in archive: {name}")

                target = (root / Path(*path.parts)).resolve()
                if os.path.commonpath([str(root), str(target)]) != str(root):
                    raise InstallerError(f"Unsafe path in archive: {name}")

            archive.extractall(root)
    except zipfile.BadZipFile as error:
        raise InstallerError(f"Downloaded archive is not a valid zip file: {zip_path.name}") from error
    except (RuntimeError, NotImplementedError) as error:
        raise InstallerError(f"Could not extract archive {zip_path.name}: {error}") from error
    except OSError as error:
        raise InstallerError(f"Could not extract archive {zip_path.name}: {error}") from error


def find_install_artifact(extract_dir: Path) -> Path:
    apdu_files = sorted(extract_dir.rglob("minotari_ledger_wallet.apdu"))
    if apdu_files:
        return apdu_files[0]

    raise InstallerError("Archive did not contain minotari_ledger_wallet.apdu.")


def model_from_target_id(target_id: int) -> LedgerModel:
    if target_id in MODEL_BY_TARGET_ID:
        return MODEL_BY_TARGET_ID[target_id]
    if target_id in UNSUPPORTED_TARGET_IDS:
        raise InstallerError(
            f"{UNSUPPORTED_TARGET_IDS[target_id]} is not supported by Minotari. "
            "Use Nano S Plus, Nano X, Stax, or Flex."
        )
    raise InstallerError(f"Unsupported Ledger target id: 0x{target_id:08x}.")


def detect_ledger_model() -> LedgerModel:
    try:
        from ledgerwallet.client import LedgerClient, NoLedgerDeviceException
    except ImportError as error:
        raise InstallerError(
            "Ledger tooling is not installed. Run this installer through the "
            "install_minotari_ledger.sh / install_minotari_ledger.ps1 launcher "
            "(or `uv run install_minotari_ledger.py`) so dependencies are provisioned."
        ) from error

    client = None
    try:
        client = LedgerClient()
        return model_from_target_id(client.target_id)
    except NoLedgerDeviceException as error:
        raise InstallerError("No Ledger device was detected. Connect and unlock the device.") from error
    except InstallerError:
        raise
    except Exception as error:
        raise InstallerError(f"Could not query Ledger device: {error}") from error
    finally:
        if client is not None:
            client.close()


def ledgerctl_command() -> list:
    """Locate the ledgerctl console script that ships with the ledgerwallet package.

    Prefer the script next to the running interpreter (the uv-managed environment)
    and fall back to whatever is on PATH.
    """
    script_name = "ledgerctl.exe" if sys.platform == "win32" else "ledgerctl"
    candidate = Path(sys.executable).with_name(script_name)
    if candidate.exists():
        return [str(candidate)]
    return [script_name]


def remove_existing_app() -> None:
    """Best-effort removal of a previous install so a fresh load does not clash.

    A device with no prior install, or one where the user declines the on-device
    prompt, is not an error here; the subsequent install step surfaces real
    failures.
    """
    print_info(f"Removing any existing '{LEDGER_APP_NAME}' installation")
    try:
        subprocess.run(
            [*ledgerctl_command(), "delete", LEDGER_APP_NAME],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    except OSError:
        # ledgerctl unavailable or not runnable; leave it to the install step.
        pass


def install_apdu_file(apdu_path: Path, model: LedgerModel) -> None:
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "ledgerblue.runScript",
            "--scp",
            "--targetId",
            f"0x{model.target_id:08x}",
            "--fileName",
            str(apdu_path),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        output = result.stdout if isinstance(result.stdout, str) else ""
        if "Invalid status 511f" in output or "OS version on your device does not seem compatible" in output:
            raise InstallerError(
                "The Ledger firmware is too old for this Minotari Ledger app artifact. "
                "Update the Ledger device firmware in Ledger Live, unlock the device, and run this installer again."
            )
        if output:
            print(output, file=sys.stderr, end="" if output.endswith("\n") else "\n")
        raise InstallerError(f"ledgerblue runScript failed with exit code {result.returncode}.")


def download_and_install(model: LedgerModel, tag: Optional[str]) -> None:
    print_step(f"Resolving release artifact for {model.display_name}")
    asset = fetch_release_asset(model.slug, tag)
    print_info(f"Selected {asset.asset_name} from {asset.tag_name}")

    with tempfile.TemporaryDirectory(prefix="minotari-ledger-") as tmp:
        tmp_dir = Path(tmp)
        zip_path = tmp_dir / asset.asset_name
        checksum_path = tmp_dir / f"{asset.asset_name}.sha256"

        print_step("Downloading firmware archive")
        download_file(asset.download_url, zip_path)
        download_file(asset.checksum_url, checksum_path)

        try:
            checksum_text = checksum_path.read_text(encoding="utf-8")
        except UnicodeDecodeError as error:
            raise InstallerError(f"Checksum file for {asset.asset_name} is not valid UTF-8.") from error
        except OSError as error:
            raise InstallerError(f"Could not read checksum file for {asset.asset_name}: {error}") from error

        expected = parse_sha256_file(checksum_text, asset.asset_name)
        verify_sha256(zip_path, expected)
        print_info("Checksum verified")

        print_step("Extracting firmware archive")
        extract_dir = tmp_dir / "extract"
        extract_dir.mkdir()
        safe_extract(zip_path, extract_dir)
        apdu_path = find_install_artifact(extract_dir)
        print_info(f"Found APDU artifact: {apdu_path.name}")

        print_step(f"Installing Minotari Wallet on {model.display_name}")
        print_info("Keep the Ledger connected, unlocked, and approve prompts on the device.")
        remove_existing_app()
        install_apdu_file(apdu_path, model)


def run(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    _enable_windows_ansi()

    try:
        if sys.version_info < (3, 9):
            raise InstallerError(
                f"Python 3.9+ is required; found {sys.version_info.major}.{sys.version_info.minor}."
            )

        print_step("Detecting connected Ledger model")
        model = detect_ledger_model()
        print_info(f"Detected {model.display_name}")

        download_and_install(model, args.tag)
    except KeyboardInterrupt:
        print("\nInstallation interrupted.", file=sys.stderr)
        return 130
    except InstallerError as error:
        print_error(str(error))
        return 1

    print_success("Minotari Ledger Wallet installed successfully")
    return 0


def main() -> None:
    sys.exit(run(sys.argv[1:]))


if __name__ == "__main__":
    main()
