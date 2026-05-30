#!/usr/bin/env python3
"""
Unified installer for the Minotari Ledger Wallet application.

The installer detects the connected Ledger model, downloads the matching Tari
release artifact, verifies it, and installs it. It supports Nano S Plus, Nano X,
Stax, and Flex. The original Nano S is intentionally unsupported.
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
import venv
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Callable, Iterable, Optional, Sequence

GITHUB_REPO = "tari-project/tari"
GITHUB_API = f"https://api.github.com/repos/{GITHUB_REPO}"
USER_AGENT = "minotari-ledger-installer/1.0"
BOOTSTRAP_ENV = "MINOTARI_LEDGER_INSTALLER_BOOTSTRAPPED"


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


@dataclass(frozen=True)
class InstallArtifact:
    kind: str
    path: Path


class InstallerError(Exception):
    """Expected installer failure with a user-facing message."""


def print_step(message: str) -> None:
    print(f"==> {message}")


def print_info(message: str) -> None:
    print(f"    {message}")


def cache_dir() -> Path:
    if sys.platform == "win32":
        root = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
    elif sys.platform == "darwin":
        root = Path.home() / "Library" / "Caches"
    else:
        root = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
    return root / "minotari-ledger-installer"


def venv_python_path(venv_dir: Path) -> Path:
    if sys.platform == "win32":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def module_available(python: Path, module: str) -> bool:
    result = subprocess.run(
        [str(python), "-c", f"import {module}"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.returncode == 0


def ensure_bootstrapped() -> None:
    if os.environ.get(BOOTSTRAP_ENV) == "1":
        return

    venv_dir = cache_dir() / f"venv-py{sys.version_info.major}{sys.version_info.minor}"
    python = venv_python_path(venv_dir)

    try:
        if not python.exists():
            print_step(f"Creating isolated Python environment at {venv_dir}")
            venv.EnvBuilder(with_pip=True).create(venv_dir)

        if not module_available(python, "ledgerwallet"):
            print_step("Installing Ledger tooling into isolated environment")
            subprocess.check_call([str(python), "-m", "pip", "install", "--upgrade", "pip"])
            subprocess.check_call([str(python), "-m", "pip", "install", "ledgerwallet"])

        env = os.environ.copy()
        env[BOOTSTRAP_ENV] = "1"
        args = [str(python), str(Path(__file__).resolve()), *sys.argv[1:]]
        if sys.platform == "win32":
            sys.exit(subprocess.call(args, env=env))
        os.execve(str(python), args, env)
    except (OSError, subprocess.CalledProcessError) as error:
        raise InstallerError(
            f"Failed to prepare isolated Ledger tooling environment at {venv_dir}."
        ) from error


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install the Minotari Ledger Wallet app for the connected Ledger device.",
    )
    parser.add_argument(
        "-t",
        "--tag",
        help="Install a specific Tari release tag, e.g. v5.4.0-pre.1.",
    )
    parser.add_argument(
        "-m",
        "--model",
        choices=sorted(SUPPORTED_MODELS),
        help="Skip device auto-detection and install for the specified model.",
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
    zip_asset = None
    for asset in assets:
        name = asset.get("name", "")
        if asset_matches_model(name, model):
            zip_asset = asset
            break

    if zip_asset is None:
        return None

    checksum_name = f"{zip_asset['name']}.sha256"
    checksum_asset = next((asset for asset in assets if asset.get("name") == checksum_name), None)
    if checksum_asset is None:
        return None

    return ReleaseAsset(
        tag_name=release.get("tag_name", "unknown"),
        asset_name=zip_asset["name"],
        download_url=zip_asset["browser_download_url"],
        checksum_url=checksum_asset["browser_download_url"],
    )


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
    actual = sha256_file(path)
    if actual.lower() != expected_digest.lower():
        raise InstallerError(
            f"Checksum mismatch for {path.name}: expected {expected_digest}, got {actual}."
        )


def safe_extract(zip_path: Path, extract_dir: Path) -> None:
    root = extract_dir.resolve()
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


def find_install_artifact(extract_dir: Path, model: str) -> InstallArtifact:
    apdu_files = sorted(extract_dir.rglob("minotari_ledger_wallet.apdu"))
    if apdu_files:
        return InstallArtifact("apdu", apdu_files[0])

    preferred_names = [
        f"app_{model}.json",
        f"app_{model}.toml",
        "app.json",
        "app.toml",
    ]
    for name in preferred_names:
        matches = sorted(extract_dir.rglob(name))
        if matches:
            return InstallArtifact("manifest", matches[0])

    raise InstallerError(
        "Archive did not contain minotari_ledger_wallet.apdu, "
        f"app_{model}.json, app_{model}.toml, app.json, or app.toml."
    )


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
        raise InstallerError("Ledger tooling is not installed.") from error

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


def read_apdu_lines(apdu_path: Path) -> list[bytes]:
    commands = []
    for index, raw_line in enumerate(apdu_path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        try:
            commands.append(bytes.fromhex(line))
        except ValueError as error:
            raise InstallerError(f"Invalid APDU hex on line {index} of {apdu_path.name}.") from error
    if not commands:
        raise InstallerError(f"{apdu_path.name} did not contain any APDU commands.")
    return commands


def create_ledger_client():
    from ledgerwallet.client import LedgerClient

    return LedgerClient()


def apdu_status_message(index: int, status: int) -> str:
    messages = {
        0x6985: "Installation was rejected on the Ledger device.",
        0x6A80: "APDU data was invalid. The app may already be installed.",
        0x6A84: "Not enough space is available on the Ledger device.",
        0x6A85: "Not enough space is available on the Ledger device.",
    }
    return messages.get(status, f"APDU {index} failed with status 0x{status:04x}.")


def install_apdu_file(
    apdu_path: Path,
    client_factory: Callable[[], object] = create_ledger_client,
) -> None:
    commands = read_apdu_lines(apdu_path)
    try:
        client = client_factory()
    except Exception as error:
        raise InstallerError(f"Could not open Ledger device: {error}") from error
    try:
        for index, command in enumerate(commands, start=1):
            try:
                response = client.raw_exchange(command)
            except Exception as error:
                raise InstallerError(f"Ledger communication failed while sending APDU {index}: {error}") from error
            if len(response) < 2:
                raise InstallerError(f"APDU {index} returned an invalid empty response.")
            status = int.from_bytes(response[-2:], "big")
            if status != 0x9000:
                raise InstallerError(apdu_status_message(index, status))
            if index == 1 or index == len(commands) or index % 100 == 0:
                print_info(f"Sent APDU {index}/{len(commands)}")
    finally:
        close = getattr(client, "close", None)
        if callable(close):
            close()


def install_manifest(manifest_path: Path) -> None:
    print_info("Using manifest fallback for older release artifact.")
    result = subprocess.run(
        [sys.executable, "-m", "ledgerwallet.ledgerctl", "install", str(manifest_path)],
        check=False,
    )
    if result.returncode != 0:
        raise InstallerError(f"ledgerctl install failed with exit code {result.returncode}.")


def install_artifact(artifact: InstallArtifact) -> None:
    if artifact.kind == "apdu":
        install_apdu_file(artifact.path)
    elif artifact.kind == "manifest":
        install_manifest(artifact.path)
    else:
        raise InstallerError(f"Unknown install artifact kind: {artifact.kind}")


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

        expected = parse_sha256_file(checksum_path.read_text(encoding="utf-8"), asset.asset_name)
        verify_sha256(zip_path, expected)
        print_info("Checksum verified")

        print_step("Extracting firmware archive")
        extract_dir = tmp_dir / "extract"
        extract_dir.mkdir()
        safe_extract(zip_path, extract_dir)
        artifact = find_install_artifact(extract_dir, model.slug)
        print_info(f"Found {artifact.kind} artifact: {artifact.path.name}")

        print_step(f"Installing Minotari Wallet on {model.display_name}")
        print_info("Keep the Ledger connected, unlocked, and approve prompts on the device.")
        install_artifact(artifact)


def run(argv: Sequence[str]) -> int:
    args = parse_args(argv)

    try:
        if sys.version_info < (3, 9):
            raise InstallerError(
                f"Python 3.9+ is required; found {sys.version_info.major}.{sys.version_info.minor}."
            )
        ensure_bootstrapped()

        if args.model:
            model = SUPPORTED_MODELS[args.model]
            print_info(f"Using requested model: {model.display_name}")
        else:
            print_step("Detecting connected Ledger model")
            model = detect_ledger_model()
            print_info(f"Detected {model.display_name}")

        download_and_install(model, args.tag)
    except KeyboardInterrupt:
        print("\nInstallation interrupted.", file=sys.stderr)
        return 130
    except InstallerError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    print_step("Minotari Ledger Wallet installed successfully")
    return 0


def main() -> None:
    sys.exit(run(sys.argv[1:]))


if __name__ == "__main__":
    main()
