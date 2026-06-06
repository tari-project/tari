#!/usr/bin/env python3
"""Install the Minotari Ledger wallet app on the connected Ledger device."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import venv
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

GITHUB_API = "https://api.github.com/repos/tari-project/tari"
APP_ASSET_RE = r"^minotari_ledger_wallet-{model}-.*\.zip$"
DEFAULT_RELEASE_LIMIT = 20


class InstallerError(RuntimeError):
    """A user-actionable installer failure."""


@dataclass(frozen=True)
class LedgerModel:
    slug: str
    display: str
    target_id: str
    usb_product_ids: tuple[int, ...]


MODELS: dict[str, LedgerModel] = {
    "nanosplus": LedgerModel("nanosplus", "Ledger Nano S Plus", "0x33100004", (0x50, 0x0005)),
    "nanox": LedgerModel("nanox", "Ledger Nano X", "0x33000004", (0x40, 0x0004)),
    "stax": LedgerModel("stax", "Ledger Stax", "0x33200004", (0x60, 0x0006)),
    "flex": LedgerModel("flex", "Ledger Flex", "0x33300004", (0x70, 0x0007)),
}

PID_TO_MODEL = {
    product_id: model
    for model in MODELS.values()
    for product_id in model.usb_product_ids
}


def print_step(message: str) -> None:
    print(f"[minotari-ledger] {message}")


def normalize_pid(value: str | int) -> int | None:
    if isinstance(value, int):
        return value
    value = str(value).strip().lower()
    if not value:
        return None
    value = value.removeprefix("0x")
    value = value.strip()
    try:
        return int(value, 16)
    except ValueError:
        return None


def model_from_pid(value: str | int) -> LedgerModel | None:
    pid = normalize_pid(value)
    if pid is None:
        return None
    return PID_TO_MODEL.get(pid)


def parse_linux_lsusb(text: str) -> list[LedgerModel]:
    models: list[LedgerModel] = []
    for match in re.finditer(r"\bID\s+2c97:([0-9a-fA-F]{1,4})\b", text):
        model = model_from_pid(match.group(1))
        if model:
            models.append(model)
    return unique_models(models)


def parse_windows_pnp_ids(text: str) -> list[LedgerModel]:
    models: list[LedgerModel] = []
    for match in re.finditer(r"\bVID_2C97&PID_([0-9A-Fa-f]{1,4})\b", text):
        model = model_from_pid(match.group(1))
        if model:
            models.append(model)
    return unique_models(models)


def _walk_system_profiler_items(node: Any) -> Iterable[dict[str, Any]]:
    if isinstance(node, dict):
        yield node
        children = node.get("_items")
        if isinstance(children, list):
            for child in children:
                yield from _walk_system_profiler_items(child)
    elif isinstance(node, list):
        for item in node:
            yield from _walk_system_profiler_items(item)


def parse_macos_system_profiler(data: dict[str, Any]) -> list[LedgerModel]:
    models: list[LedgerModel] = []
    for item in _walk_system_profiler_items(data.get("SPUSBDataType", [])):
        vendor = normalize_pid(str(item.get("vendor_id", "")))
        if vendor != 0x2C97:
            continue
        product = item.get("product_id")
        if product is None:
            continue
        model = model_from_pid(str(product))
        if model:
            models.append(model)
    return unique_models(models)


def unique_models(models: Iterable[LedgerModel]) -> list[LedgerModel]:
    seen: set[str] = set()
    result: list[LedgerModel] = []
    for model in models:
        if model.slug in seen:
            continue
        seen.add(model.slug)
        result.append(model)
    return result


def detect_connected_models() -> list[LedgerModel]:
    if sys.platform == "win32":
        cmd = [
            "powershell",
            "-NoProfile",
            "-Command",
            (
                "Get-PnpDevice -PresentOnly | "
                "Where-Object { $_.InstanceId -match 'VID_2C97' } | "
                "Select-Object -ExpandProperty InstanceId"
            ),
        ]
        result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        return parse_windows_pnp_ids(result.stdout if result.returncode == 0 else "")

    if sys.platform == "darwin":
        result = subprocess.run(
            ["system_profiler", "SPUSBDataType", "-json"],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        if result.returncode != 0 or not result.stdout.strip():
            return []
        try:
            data = json.loads(result.stdout)
        except json.JSONDecodeError:
            return []
        return parse_macos_system_profiler(data)

    lsusb = shutil.which("lsusb")
    if not lsusb:
        return []
    result = subprocess.run(
        [lsusb, "-d", "2c97:"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    return parse_linux_lsusb(result.stdout if result.returncode == 0 else "")


def select_model(requested: str | None) -> LedgerModel:
    if requested:
        try:
            return MODELS[requested]
        except KeyError as exc:
            raise InstallerError(f"Unsupported Ledger model: {requested}") from exc

    models = detect_connected_models()
    if len(models) == 1:
        return models[0]
    if len(models) > 1:
        names = ", ".join(model.slug for model in models)
        raise InstallerError(
            f"Multiple Ledger devices detected ({names}); pass --model explicitly."
        )
    supported = ", ".join(sorted(MODELS))
    raise InstallerError(
        "Could not auto-detect a supported Ledger device over USB. "
        f"Connect one device on the dashboard, or pass --model ({supported})."
    )


def github_json(path: str) -> Any:
    request = urllib.request.Request(
        f"{GITHUB_API}{path}",
        headers={"Accept": "application/vnd.github+json", "User-Agent": "minotari-ledger-installer"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raise InstallerError(f"GitHub request failed for {path}: HTTP {exc.code}") from exc
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise InstallerError(f"GitHub request failed for {path}: {exc}") from exc


@dataclass(frozen=True)
class ReleaseAsset:
    tag: str
    name: str
    url: str
    checksum_name: str | None
    checksum_url: str | None


def find_asset_pair(release: dict[str, Any], model: LedgerModel) -> ReleaseAsset | None:
    pattern = re.compile(APP_ASSET_RE.format(model=re.escape(model.slug)))
    assets = release.get("assets") or []
    zip_asset = next((asset for asset in assets if pattern.match(asset.get("name", ""))), None)
    if not zip_asset:
        return None

    checksum_name = f"{zip_asset['name']}.sha256"
    checksum_asset = next((asset for asset in assets if asset.get("name") == checksum_name), None)
    return ReleaseAsset(
        tag=str(release.get("tag_name") or ""),
        name=zip_asset["name"],
        url=zip_asset["browser_download_url"],
        checksum_name=checksum_asset.get("name") if checksum_asset else None,
        checksum_url=checksum_asset.get("browser_download_url") if checksum_asset else None,
    )


def resolve_release_asset(model: LedgerModel, tag: str | None = None) -> ReleaseAsset:
    if tag:
        release = github_json(f"/releases/tags/{tag}")
        asset = find_asset_pair(release, model)
        if not asset:
            raise InstallerError(f"Release {tag} has no {model.slug} Minotari Ledger zip asset.")
        return asset

    releases = github_json(f"/releases?per_page={DEFAULT_RELEASE_LIMIT}")
    for release in releases:
        if release.get("draft"):
            continue
        asset = find_asset_pair(release, model)
        if asset:
            return asset
    raise InstallerError(
        f"No recent non-draft release contains a {model.slug} Minotari Ledger zip asset."
    )


def download_file(url: str, destination: Path) -> Path:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "minotari-ledger-installer"})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            total_text = response.headers.get("Content-Length", "0")
            try:
                total = int(total_text)
            except ValueError:
                total = 0
            hasher = hashlib.sha256()
            read_bytes = 0
            with destination.open("wb") as handle:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    read_bytes += len(chunk)
                    hasher.update(chunk)
                    handle.write(chunk)
            print_step(
                f"downloaded {destination.name}"
                + (f" ({read_bytes}/{total} bytes)" if total else f" ({read_bytes} bytes)")
            )
    except (urllib.error.URLError, TimeoutError) as exc:
        raise InstallerError(f"Failed to download {url}: {exc}") from exc
    return destination


def parse_sha256_file(text: str, expected_filename: str) -> str:
    unnamed: list[str] = []
    saw_named = False
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.replace("*", " ").split()
        if len(parts) == 1 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            unnamed.append(parts[0].lower())
            continue
        if len(parts) >= 2 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            saw_named = True
            filename = Path(parts[-1]).name
            if filename == expected_filename:
                return parts[0].lower()

    if not saw_named and len(unnamed) == 1:
        return unnamed[0]
    raise InstallerError(f"Checksum file does not contain a digest for {expected_filename}.")


def verify_sha256(path: Path, expected_digest: str) -> None:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    actual = hasher.hexdigest()
    if actual.lower() != expected_digest.lower():
        raise InstallerError(
            f"Checksum mismatch for {path.name}: expected {expected_digest}, got {actual}."
        )
    print_step(f"verified sha256 for {path.name}")


def safe_extract_zip(zip_path: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    root = destination.resolve()
    with zipfile.ZipFile(zip_path) as archive:
        for member in archive.infolist():
            target = (destination / member.filename).resolve()
            if target != root and root not in target.parents:
                raise InstallerError(f"Refusing unsafe archive member: {member.filename}")
        archive.extractall(destination)


@dataclass(frozen=True)
class InstallPayload:
    apdu: Path
    elf: Path | None


def locate_install_payload(directory: Path) -> InstallPayload:
    apdus = sorted(directory.rglob("*.apdu"))
    if not apdus:
        raise InstallerError("No .apdu install script found after extracting the release asset.")
    apdu = next((path for path in apdus if path.name == "minotari_ledger_wallet.apdu"), apdus[0])
    elf = next(iter(sorted(directory.rglob("*.elf"))), None)
    return InstallPayload(apdu=apdu, elf=elf)


def default_venv_dir() -> Path:
    return Path.home() / ".tari" / "minotari-ledger-installer" / "venv"


def venv_python(venv_dir: Path) -> Path:
    if sys.platform == "win32":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def venv_has_ledgerblue(python: Path) -> bool:
    result = subprocess.run(
        [str(python), "-c", "import ledgerblue"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    return result.returncode == 0


def ensure_bootstrap_venv(venv_dir: Path) -> Path:
    python = venv_python(venv_dir)
    created = False
    if not python.exists():
        print_step(f"creating Python virtual environment at {venv_dir}")
        venv.EnvBuilder(with_pip=True).create(venv_dir)
        created = True
    if not created and venv_has_ledgerblue(python):
        return python
    print_step("installing Ledger loader dependencies into isolated venv")
    subprocess.run(
        [str(python), "-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel"],
        check=True,
    )
    subprocess.run([str(python), "-m", "pip", "install", "ledgerblue"], check=True)
    return python


def build_install_command(python: Path, payload: InstallPayload, model: LedgerModel) -> list[str]:
    command = [
        str(python),
        "-m",
        "ledgerblue.runScript",
        "--scp",
        "--fileName",
        str(payload.apdu),
    ]
    if payload.elf:
        command.extend(["--elfFile", str(payload.elf)])
    else:
        command.extend(["--targetId", model.target_id])
    return command


def run_install(command: list[str], dry_run: bool) -> None:
    print_step("install command:")
    print(" ".join(command))
    if dry_run:
        print_step("dry run requested; not sending APDUs to the Ledger device")
        return
    subprocess.run(command, check=True)


def install(args: argparse.Namespace) -> None:
    model = select_model(args.model)
    print_step(f"selected {model.display}")
    asset = resolve_release_asset(model, args.tag)
    print_step(f"selected release {asset.tag}: {asset.name}")

    cleanup_dir: Path | None = None
    if args.download_dir:
        download_dir = Path(args.download_dir)
    else:
        cleanup_dir = Path(tempfile.mkdtemp(prefix="minotari-ledger-"))
        download_dir = cleanup_dir

    try:
        archive_path = download_file(asset.url, download_dir / asset.name)
        if not asset.checksum_url:
            raise InstallerError(f"Missing checksum sidecar for {asset.name}.")
        checksum_path = download_file(asset.checksum_url, download_dir / asset.checksum_name)
        digest = parse_sha256_file(checksum_path.read_text(encoding="utf-8"), asset.name)
        verify_sha256(archive_path, digest)

        extract_dir = download_dir / archive_path.stem
        safe_extract_zip(archive_path, extract_dir)
        payload = locate_install_payload(extract_dir)
        print_step(f"using APDU script {payload.apdu}")

        python = Path(sys.executable) if args.no_bootstrap else ensure_bootstrap_venv(Path(args.venv_dir))
        command = build_install_command(python, payload, model)
        run_install(command, args.dry_run)
        if not args.dry_run:
            print_step("Minotari Ledger wallet installed successfully.")
    finally:
        if cleanup_dir:
            shutil.rmtree(cleanup_dir, ignore_errors=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Auto-detect a Ledger device and install the matching Minotari wallet app.",
    )
    parser.add_argument("--model", choices=sorted(MODELS), help="override USB auto-detection")
    parser.add_argument("--tag", help="install a specific Tari release tag")
    parser.add_argument("--download-dir", help="keep downloads and extraction under this directory")
    parser.add_argument("--venv-dir", default=str(default_venv_dir()), help="isolated venv path")
    parser.add_argument("--no-bootstrap", action="store_true", help="use the current Python env")
    parser.add_argument("--dry-run", action="store_true", help="download and validate without install")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        install(args)
        return 0
    except subprocess.CalledProcessError as exc:
        print(f"error: command failed with exit code {exc.returncode}: {exc.cmd}", file=sys.stderr)
        return exc.returncode or 1
    except InstallerError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
