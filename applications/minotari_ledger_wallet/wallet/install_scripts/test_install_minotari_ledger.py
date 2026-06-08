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


class TestBootstrapHandling(unittest.TestCase):
    def test_windows_bootstrap_waits_for_child_process(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            python = Path(temp_dir) / "python.exe"
            python.touch()
            with mock.patch.object(installer, "cache_dir", return_value=Path(temp_dir)), \
                mock.patch.object(installer, "venv_python_path", return_value=python), \
                mock.patch.object(installer, "module_available", return_value=True), \
                mock.patch.object(installer.sys, "platform", "win32"), \
                mock.patch.object(installer.sys, "argv", ["install_minotari_ledger.py", "--model", "flex"]), \
                mock.patch.object(installer.subprocess, "call", return_value=7) as call, \
                mock.patch.dict(installer.os.environ, {}, clear=True):
                with self.assertRaises(SystemExit) as context:
                    installer.ensure_bootstrapped()

            self.assertEqual(context.exception.code, 7)
            self.assertEqual(call.call_args.args[0], [str(python), str(SCRIPT_PATH.resolve()), "--model", "flex"])

    def test_bootstrap_installs_missing_ledgerblue(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            python = Path(temp_dir) / "python"
            python.touch()

            def module_available(_python, module):
                return module == "ledgerwallet"

            with mock.patch.object(installer, "cache_dir", return_value=Path(temp_dir)), \
                mock.patch.object(installer, "venv_python_path", return_value=python), \
                mock.patch.object(installer, "module_available", side_effect=module_available), \
                mock.patch.object(installer.subprocess, "check_call") as check_call, \
                mock.patch.object(installer.os, "execve", side_effect=SystemExit(0)), \
                mock.patch.object(installer.sys, "platform", "linux"), \
                mock.patch.object(installer.sys, "argv", ["install_minotari_ledger.py", "--model", "flex"]), \
                mock.patch.object(installer, "print_step"), \
                mock.patch.dict(installer.os.environ, {}, clear=True):
                with self.assertRaises(SystemExit):
                    installer.ensure_bootstrapped()

            self.assertEqual(
                check_call.call_args_list[-1].args[0],
                [str(python), "-m", "pip", "install", "ledgerblue"],
            )


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
    def test_parse_sha256_accepts_single_bare_digest(self):
        checksum = "a" * 64

        self.assertEqual(installer.parse_sha256_file(f"{checksum}\n", "firmware.zip"), checksum)

    def test_parse_sha256_for_expected_filename(self):
        checksum = "a" * 64
        text = f"{'b' * 64}  other.zip\n{checksum}  firmware.zip\n"

        self.assertEqual(installer.parse_sha256_file(text, "firmware.zip"), checksum)

    def test_parse_sha256_rejects_single_hash_for_wrong_filename(self):
        text = f"{'a' * 64}  other.zip\n"

        with self.assertRaisesRegex(installer.InstallerError, "firmware.zip"):
            installer.parse_sha256_file(text, "firmware.zip")

    def test_parse_sha256_rejects_multiple_hashes_without_expected_filename(self):
        text = f"{'a' * 64}  one.zip\n{'b' * 64}  two.zip\n"

        with self.assertRaisesRegex(installer.InstallerError, "firmware.zip"):
            installer.parse_sha256_file(text, "firmware.zip")

    def test_parse_sha256_rejects_bare_digest_mixed_with_named_mismatch(self):
        text = f"{'a' * 64}  other.zip\n{'b' * 64}\n"

        with self.assertRaisesRegex(installer.InstallerError, "firmware.zip"):
            installer.parse_sha256_file(text, "firmware.zip")

    def test_checksum_mismatch_raises(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "firmware.zip"
            path.write_bytes(b"firmware")

            with self.assertRaisesRegex(installer.InstallerError, "Checksum mismatch"):
                installer.verify_sha256(path, "0" * 64)


class FakeDownloadResponse:
    def __init__(self, headers, chunks):
        self.headers = headers
        self.chunks = list(chunks)

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self, _size=-1):
        if not self.chunks:
            return b""
        return self.chunks.pop(0)


class TestDownloadHandling(unittest.TestCase):
    def test_download_file_ignores_invalid_content_length(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            destination = Path(temp_dir) / "firmware.zip"
            response = FakeDownloadResponse({"Content-Length": "not-a-number"}, [b"firm", b"ware"])

            with mock.patch.object(installer.urllib.request, "urlopen", return_value=response):
                installer.download_file("https://example.com/firmware.zip", destination)

            self.assertEqual(destination.read_bytes(), b"firmware")


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

    def test_generic_manifest_fallback(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "app.json").write_text("{}", encoding="utf-8")

            artifact = installer.find_install_artifact(root, "nanox")

            self.assertEqual(artifact.kind, "manifest")
            self.assertEqual(artifact.path.name, "app.json")

    def test_manifest_fallback_rejects_other_model_manifest(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "app_stax.json").write_text("{}", encoding="utf-8")

            with self.assertRaisesRegex(installer.InstallerError, "app_nanox"):
                installer.find_install_artifact(root, "nanox")


class TestApduInstall(unittest.TestCase):
    def test_apdu_install_uses_ledgerblue_secure_channel(self):
        apdu = Path("minotari_ledger_wallet.apdu")
        completed = mock.Mock(returncode=0)

        with mock.patch.object(installer.subprocess, "run", return_value=completed) as run:
            installer.install_apdu_file(apdu, installer.SUPPORTED_MODELS["nanosplus"])

        self.assertEqual(
            run.call_args.args[0],
            [
                installer.sys.executable,
                "-m",
                "ledgerblue.runScript",
                "--scp",
                "--targetId",
                "0x33100004",
                "--fileName",
                str(apdu),
            ],
        )
        self.assertFalse(run.call_args.kwargs["check"])

    def test_apdu_install_failure_is_user_facing(self):
        completed = mock.Mock(returncode=7)

        with mock.patch.object(installer.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(installer.InstallerError, "ledgerblue runScript failed"):
                installer.install_apdu_file(Path("app.apdu"), installer.SUPPORTED_MODELS["flex"])

    def test_apdu_install_reports_old_firmware(self):
        completed = mock.Mock(
            returncode=1,
            stdout=(
                "ledgerblue.commException.CommException: Exception : Invalid status 511f "
                "(The OS version on your device does not seem compatible with the SDK version used to build the app)"
            ),
        )

        with mock.patch.object(installer.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(installer.InstallerError, "firmware is too old"):
                installer.install_apdu_file(Path("app.apdu"), installer.SUPPORTED_MODELS["nanosplus"])


if __name__ == "__main__":
    unittest.main()
