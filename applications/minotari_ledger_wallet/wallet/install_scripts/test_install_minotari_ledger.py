#!/usr/bin/env python3

import importlib.util
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock


SCRIPT_PATH = Path(__file__).with_name("install_minotari_ledger.py")
SPEC = importlib.util.spec_from_file_location("install_minotari_ledger", SCRIPT_PATH)
installer = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules["install_minotari_ledger"] = installer
SPEC.loader.exec_module(installer)


class TestModelMapping(unittest.TestCase):
    def test_supported_target_ids(self):
        self.assertEqual(installer.model_from_target_id(0x33100004).slug, "nanosplus")
        self.assertEqual(installer.model_from_target_id(0x33000004).slug, "nanox")
        self.assertEqual(installer.model_from_target_id(0x33200004).slug, "stax")
        self.assertEqual(installer.model_from_target_id(0x33300004).slug, "flex")

    def test_original_nano_s_is_unsupported(self):
        with self.assertRaisesRegex(installer.InstallerError, "Nano S is not supported"):
            installer.model_from_target_id(0x31100004)


class TestReleaseSelection(unittest.TestCase):
    def test_selects_matching_asset_and_checksum(self):
        release = {
            "tag_name": "v5.4.0-pre.1",
            "assets": [
                {
                    "name": "minotari_ledger_wallet-nanosplus-v5.4.0-pre.1-abc1234.zip",
                    "browser_download_url": "https://example.com/nanosplus.zip",
                },
                {
                    "name": "minotari_ledger_wallet-nanosplus-v5.4.0-pre.1-abc1234.zip.sha256",
                    "browser_download_url": "https://example.com/nanosplus.zip.sha256",
                },
            ],
        }

        asset = installer.find_asset_for_model(release, "nanosplus")

        self.assertIsNotNone(asset)
        self.assertEqual(asset.tag_name, "v5.4.0-pre.1")
        self.assertEqual(asset.download_url, "https://example.com/nanosplus.zip")
        self.assertEqual(asset.checksum_url, "https://example.com/nanosplus.zip.sha256")

    def test_default_selection_skips_non_matching_newer_release(self):
        stable_without_ledger_assets = {
            "tag_name": "v5.3.1",
            "draft": False,
            "assets": [{"name": "tari_suite-v5.3.1-linux.zip"}],
        }
        prerelease_with_ledger_asset = {
            "tag_name": "v5.4.0-pre.1",
            "draft": False,
            "assets": [
                {
                    "name": "minotari_ledger_wallet-flex-v5.4.0-pre.1-abc1234.zip",
                    "browser_download_url": "https://example.com/flex.zip",
                },
                {
                    "name": "minotari_ledger_wallet-flex-v5.4.0-pre.1-abc1234.zip.sha256",
                    "browser_download_url": "https://example.com/flex.zip.sha256",
                },
            ],
        }

        asset = installer.select_asset_from_releases(
            [stable_without_ledger_assets, prerelease_with_ledger_asset],
            "flex",
        )

        self.assertEqual(asset.tag_name, "v5.4.0-pre.1")

    def test_missing_checksum_is_not_selectable(self):
        release = {
            "tag_name": "v5.4.0-pre.1",
            "assets": [
                {
                    "name": "minotari_ledger_wallet-stax-v5.4.0-pre.1-abc1234.zip",
                    "browser_download_url": "https://example.com/stax.zip",
                },
            ],
        }

        self.assertIsNone(installer.find_asset_for_model(release, "stax"))


class TestChecksumHandling(unittest.TestCase):
    def test_parse_sha256_for_expected_filename(self):
        checksum = "a" * 64
        text = f"{'b' * 64}  other.zip\n{checksum}  firmware.zip\n"

        self.assertEqual(installer.parse_sha256_file(text, "firmware.zip"), checksum)

    def test_parse_sha256_rejects_multiple_hashes_without_expected_filename(self):
        text = f"{'a' * 64}  one.zip\n{'b' * 64}  two.zip\n"

        with self.assertRaisesRegex(installer.InstallerError, "firmware.zip"):
            installer.parse_sha256_file(text, "firmware.zip")

    def test_checksum_mismatch_raises(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "firmware.zip"
            path.write_bytes(b"firmware")

            with self.assertRaisesRegex(installer.InstallerError, "Checksum mismatch"):
                installer.verify_sha256(path, "0" * 64)


class TestExtractionAndArtifactSelection(unittest.TestCase):
    def test_safe_extract_accepts_normal_archive(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive = root / "firmware.zip"
            output = root / "out"
            output.mkdir()
            with zipfile.ZipFile(archive, "w") as zip_file:
                zip_file.writestr("minotari_ledger_wallet.apdu", "e0000000009000\n")

            installer.safe_extract(archive, output)

            self.assertTrue((output / "minotari_ledger_wallet.apdu").exists())

    def test_safe_extract_blocks_zip_slip(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            archive = root / "firmware.zip"
            output = root / "out"
            output.mkdir()
            with zipfile.ZipFile(archive, "w") as zip_file:
                zip_file.writestr("../evil.txt", "bad")

            with self.assertRaisesRegex(installer.InstallerError, "Unsafe path"):
                installer.safe_extract(archive, output)

    def test_apdu_artifact_is_preferred_over_manifest(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "minotari_ledger_wallet.apdu").write_text("e0000000009000\n", encoding="utf-8")
            (root / "app_nanosplus.json").write_text("{}", encoding="utf-8")

            artifact = installer.find_install_artifact(root, "nanosplus")

            self.assertEqual(artifact.kind, "apdu")
            self.assertEqual(artifact.path.name, "minotari_ledger_wallet.apdu")

    def test_manifest_fallback(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "app_nanox.json").write_text("{}", encoding="utf-8")

            artifact = installer.find_install_artifact(root, "nanox")

            self.assertEqual(artifact.kind, "manifest")
            self.assertEqual(artifact.path.name, "app_nanox.json")


class FakeLedgerClient:
    def __init__(self, responses):
        self.responses = list(responses)
        self.commands = []
        self.closed = False

    def raw_exchange(self, command):
        self.commands.append(command)
        return self.responses.pop(0)

    def close(self):
        self.closed = True


class RaisingLedgerClient:
    def __init__(self, error):
        self.error = error
        self.closed = False

    def raw_exchange(self, _command):
        raise self.error

    def close(self):
        self.closed = True


class TestApduInstall(unittest.TestCase):
    def test_apdu_commands_are_sent_and_closed(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            apdu = Path(temp_dir) / "minotari_ledger_wallet.apdu"
            apdu.write_text("e000000000\n# comment\n e001000000 \n", encoding="utf-8")
            client = FakeLedgerClient([b"\x90\x00", b"\x90\x00"])

            with mock.patch.object(installer, "print_info"):
                installer.install_apdu_file(apdu, client_factory=lambda: client)

            self.assertEqual(client.commands, [bytes.fromhex("e000000000"), bytes.fromhex("e001000000")])
            self.assertTrue(client.closed)

    def test_apdu_non_success_status_raises(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            apdu = Path(temp_dir) / "minotari_ledger_wallet.apdu"
            apdu.write_text("e000000000\n", encoding="utf-8")
            client = FakeLedgerClient([b"\x69\x85"])

            with self.assertRaisesRegex(installer.InstallerError, "rejected"):
                installer.install_apdu_file(apdu, client_factory=lambda: client)

            self.assertTrue(client.closed)

    def test_client_open_failure_is_user_facing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            apdu = Path(temp_dir) / "minotari_ledger_wallet.apdu"
            apdu.write_text("e000000000\n", encoding="utf-8")

            def open_client():
                raise RuntimeError("usb")

            with self.assertRaisesRegex(installer.InstallerError, "Could not open Ledger device"):
                installer.install_apdu_file(apdu, client_factory=open_client)

    def test_apdu_transport_failure_is_user_facing_and_closed(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            apdu = Path(temp_dir) / "minotari_ledger_wallet.apdu"
            apdu.write_text("e000000000\n", encoding="utf-8")
            client = RaisingLedgerClient(RuntimeError("disconnected"))

            with self.assertRaisesRegex(installer.InstallerError, "Ledger communication failed"):
                installer.install_apdu_file(apdu, client_factory=lambda: client)

            self.assertTrue(client.closed)


if __name__ == "__main__":
    unittest.main()
