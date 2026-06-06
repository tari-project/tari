#!/usr/bin/env python3
"""Unit tests for the Minotari Ledger installer."""

from __future__ import annotations

import argparse
import importlib.util
import io
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import Mock, patch

SCRIPT_PATH = Path(__file__).with_name("install_minotari_ledger.py")
SPEC = importlib.util.spec_from_file_location("install_minotari_ledger", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
installer = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = installer
SPEC.loader.exec_module(installer)


class TestLedgerDeviceDetection(unittest.TestCase):
    def test_parse_linux_lsusb_detects_supported_model(self):
        text = "Bus 001 Device 002: ID 2c97:0050 Ledger Nano S Plus\n"
        self.assertEqual([model.slug for model in installer.parse_linux_lsusb(text)], ["nanosplus"])

    def test_parse_windows_pnp_ids_detects_bootloader_pid(self):
        text = r"USB\VID_2C97&PID_0007\123456"
        self.assertEqual([model.slug for model in installer.parse_windows_pnp_ids(text)], ["flex"])

    def test_parse_macos_system_profiler_walks_nested_items(self):
        data = {
            "SPUSBDataType": [
                {
                    "_name": "USB 3.1 Bus",
                    "_items": [
                        {
                            "_name": "Hub",
                            "_items": [
                                {
                                    "_name": "Ledger Stax",
                                    "vendor_id": "0x2c97",
                                    "product_id": "0x0060",
                                }
                            ],
                        }
                    ],
                }
            ]
        }
        self.assertEqual(
            [model.slug for model in installer.parse_macos_system_profiler(data)],
            ["stax"],
        )

    def test_select_model_rejects_multiple_detected_devices_without_override(self):
        with patch.object(
            installer,
            "detect_connected_models",
            return_value=[installer.MODELS["nanox"], installer.MODELS["stax"]],
        ):
            with self.assertRaisesRegex(installer.InstallerError, "Multiple Ledger devices"):
                installer.select_model(None)


class TestReleaseSelection(unittest.TestCase):
    def test_resolve_release_asset_skips_releases_without_model_asset(self):
        releases = [
            {"tag_name": "v5.3.1", "draft": False, "assets": []},
            {
                "tag_name": "v5.4.0-pre.2",
                "draft": False,
                "assets": [
                    {
                        "name": "minotari_ledger_wallet-nanosplus-v5.4.0-pre.2.zip",
                        "browser_download_url": "https://example.test/app.zip",
                    },
                    {
                        "name": "minotari_ledger_wallet-nanosplus-v5.4.0-pre.2.zip.sha256",
                        "browser_download_url": "https://example.test/app.zip.sha256",
                    },
                ],
            },
        ]
        with patch.object(installer, "github_json", return_value=releases):
            asset = installer.resolve_release_asset(installer.MODELS["nanosplus"])
        self.assertEqual(asset.tag, "v5.4.0-pre.2")
        self.assertEqual(asset.checksum_name, "minotari_ledger_wallet-nanosplus-v5.4.0-pre.2.zip.sha256")


class TestChecksumHandling(unittest.TestCase):
    def test_parse_sha256_accepts_matching_named_line(self):
        digest = "a" * 64
        text = f"{digest}  minotari_ledger_wallet-nanox.zip\n"
        self.assertEqual(installer.parse_sha256_file(text, "minotari_ledger_wallet-nanox.zip"), digest)

    def test_parse_sha256_rejects_wrong_named_file(self):
        text = f"{'b' * 64}  other.zip\n"
        with self.assertRaisesRegex(installer.InstallerError, "does not contain"):
            installer.parse_sha256_file(text, "expected.zip")

    def test_parse_sha256_accepts_single_unnamed_digest(self):
        digest = "c" * 64
        self.assertEqual(installer.parse_sha256_file(digest, "expected.zip"), digest)


class TestDownloadsAndArchives(unittest.TestCase):
    def test_download_file_handles_invalid_content_length(self):
        payload = b"abc"

        class FakeResponse(io.BytesIO):
            headers = {"Content-Length": "not-an-int"}

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

        with tempfile.TemporaryDirectory() as temp:
            destination = Path(temp) / "asset.zip"
            with patch.object(installer.urllib.request, "urlopen", return_value=FakeResponse(payload)):
                installer.download_file("https://example.test/asset.zip", destination)
            self.assertEqual(destination.read_bytes(), payload)

    def test_safe_extract_zip_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive = root / "bad.zip"
            with zipfile.ZipFile(archive, "w") as handle:
                handle.writestr("../evil.txt", "bad")
            with self.assertRaisesRegex(installer.InstallerError, "unsafe archive member"):
                installer.safe_extract_zip(archive, root / "extract")


class TestInstallCommand(unittest.TestCase):
    def test_build_install_command_uses_target_id_without_elf(self):
        payload = installer.InstallPayload(apdu=Path("app.apdu"), elf=None)
        command = installer.build_install_command(Path("python"), payload, installer.MODELS["flex"])
        self.assertIn("--targetId", command)
        self.assertIn("0x33300004", command)
        self.assertIn("--scp", command)

    def test_build_install_command_prefers_elf_file_when_present(self):
        payload = installer.InstallPayload(apdu=Path("app.apdu"), elf=Path("app.elf"))
        command = installer.build_install_command(Path("python"), payload, installer.MODELS["stax"])
        self.assertIn("--elfFile", command)
        self.assertIn("app.elf", command)
        self.assertNotIn("--targetId", command)


class TestBootstrapAndCleanup(unittest.TestCase):
    def test_ensure_bootstrap_venv_skips_pip_when_ledgerblue_is_present(self):
        with tempfile.TemporaryDirectory() as temp:
            python = Path(temp) / "python"
            python.write_text("", encoding="utf-8")
            completed = Mock(returncode=0)
            with (
                patch.object(installer, "venv_python", return_value=python),
                patch.object(installer.subprocess, "run", return_value=completed) as run,
            ):
                self.assertEqual(installer.ensure_bootstrap_venv(Path(temp)), python)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][1:], ["-c", "import ledgerblue"])

    def test_ensure_bootstrap_venv_installs_when_ledgerblue_is_missing(self):
        with tempfile.TemporaryDirectory() as temp:
            python = Path(temp) / "python"
            python.write_text("", encoding="utf-8")
            missing = Mock(returncode=1)
            installed = Mock(returncode=0)
            with (
                patch.object(installer, "venv_python", return_value=python),
                patch.object(
                    installer.subprocess,
                    "run",
                    side_effect=[missing, installed, installed],
                ) as run,
            ):
                self.assertEqual(installer.ensure_bootstrap_venv(Path(temp)), python)
            self.assertEqual(run.call_count, 3)
            self.assertIn("ledgerblue", run.call_args_list[-1].args[0])

    def test_install_removes_implicit_temporary_download_dir(self):
        with tempfile.TemporaryDirectory() as temp:
            temp_root = Path(temp)
            download_root = temp_root / "minotari-ledger-test"
            checksum_text = "a" * 64
            asset = installer.ReleaseAsset(
                tag="v1.2.3",
                name="minotari_ledger_wallet-flex-v1.2.3.zip",
                url="https://example.test/app.zip",
                checksum_name="minotari_ledger_wallet-flex-v1.2.3.zip.sha256",
                checksum_url="https://example.test/app.zip.sha256",
            )

            def fake_download(_url: str, destination: Path) -> Path:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_text(checksum_text, encoding="utf-8")
                return destination

            args = argparse.Namespace(
                model="flex",
                tag=None,
                download_dir=None,
                venv_dir=str(temp_root / "venv"),
                no_bootstrap=True,
                dry_run=True,
            )
            with (
                patch.object(installer.tempfile, "mkdtemp", return_value=str(download_root)),
                patch.object(installer, "select_model", return_value=installer.MODELS["flex"]),
                patch.object(installer, "resolve_release_asset", return_value=asset),
                patch.object(installer, "download_file", side_effect=fake_download),
                patch.object(installer, "verify_sha256"),
                patch.object(installer, "safe_extract_zip"),
                patch.object(
                    installer,
                    "locate_install_payload",
                    return_value=installer.InstallPayload(apdu=Path("app.apdu"), elf=None),
                ),
                patch.object(installer, "run_install"),
            ):
                installer.install(args)
            self.assertFalse(download_root.exists())


if __name__ == "__main__":
    unittest.main()
