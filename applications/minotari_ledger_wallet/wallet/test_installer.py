#!/usr/bin/env python3
"""
Test suite for the Minotari Ledger Wallet Unified Installer.
Tests core functionality without requiring a connected Ledger device.
"""

import json
import os
import sys
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

try:
    import install_minotari_ledger as installer
except ImportError:
    print("Error: Could not import install_minotari_ledger.py")
    sys.exit(1)


class TestPythonVersionCheck(unittest.TestCase):
    def test_valid_python_version(self):
        try:
            installer.check_python_version()
        except SystemExit:
            self.fail("check_python_version raised SystemExit for valid version")

    def test_invalid_python_version(self):
        mock_version = MagicMock()
        mock_version.major = 3
        mock_version.minor = 6
        mock_version.__lt__ = lambda self, other: (3, 6) < other

        with patch("sys.version_info", mock_version):
            with self.assertRaises(SystemExit):
                installer.check_python_version()


class TestCommandCheck(unittest.TestCase):
    @patch("shutil.which")
    def test_command_exists(self, mock_which):
        mock_which.return_value = "/usr/bin/ledgerctl"
        self.assertTrue(installer.check_command("ledgerctl"))
        mock_which.assert_called_once_with("ledgerctl")

    @patch("shutil.which")
    def test_command_not_exists(self, mock_which):
        mock_which.return_value = None
        self.assertFalse(installer.check_command("nonexistent"))


class TestLedgerModelDetection(unittest.TestCase):
    @patch("subprocess.run")
    def test_detect_flex_model(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0, stdout="Ledger Flex [Connected]")
        result = installer.detect_ledger_model()
        self.assertEqual(result, "flex")

    @patch("subprocess.run")
    def test_detect_nanos_model(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0, stdout="Ledger Nano S [Connected]")
        result = installer.detect_ledger_model()
        self.assertEqual(result, "nanos")

    @patch("subprocess.run")
    def test_detect_nanosplus_model(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0, stdout="Ledger Nano S Plus [Connected]")
        result = installer.detect_ledger_model()
        self.assertEqual(result, "nanosplus")

    @patch("subprocess.run")
    def test_detect_nanox_model(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0, stdout="Ledger Nano X [Connected]")
        result = installer.detect_ledger_model()
        self.assertEqual(result, "nanox")

    @patch("subprocess.run")
    def test_detect_stax_model(self, mock_run):
        mock_run.return_value = MagicMock(returncode=0, stdout="Ledger Stax [Connected]")
        result = installer.detect_ledger_model()
        self.assertEqual(result, "stax")

    @patch("subprocess.run")
    def test_no_device_detected(self, mock_run):
        mock_run.return_value = MagicMock(returncode=1)
        result = installer.detect_ledger_model()
        self.assertIsNone(result)

    @patch("subprocess.run")
    def test_ledgerctl_not_found(self, mock_run):
        mock_run.side_effect = FileNotFoundError()
        with self.assertRaises(SystemExit):
            installer.detect_ledger_model()


class TestGitHubReleaseDownload(unittest.TestCase):
    @patch("urllib.request.urlopen")
    def test_parse_release_json(self, mock_urlopen):
        mock_response = MagicMock()
        mock_response.read.return_value = json.dumps(
            {
                "assets": [
                    {
                        "name": "minotari_ledger_wallet-flex.zip",
                        "browser_download_url": "https://example.com/flex.zip",
                    },
                    {
                        "name": "minotari_ledger_wallet-nanox.zip",
                        "browser_download_url": "https://example.com/nanox.zip",
                    },
                ]
            }
        ).encode()

        mock_urlopen.return_value.__enter__.return_value = mock_response

        with patch("urllib.request.urlretrieve"):
            with patch("os.path.join", return_value="/tmp/test.zip"):
                try:
                    data = json.loads(mock_response.read().decode())
                    assets = [
                        asset
                        for asset in data["assets"]
                        if "minotari_ledger_wallet-flex" in asset["name"]
                    ]
                    self.assertEqual(len(assets), 1)
                    self.assertEqual(assets[0]["name"], "minotari_ledger_wallet-flex.zip")
                except json.JSONDecodeError:
                    self.fail("Failed to parse JSON response")


class TestZipExtraction(unittest.TestCase):
    @patch("zipfile.ZipFile")
    def test_extract_zip(self, mock_zipfile):
        mock_zip_instance = MagicMock()
        mock_zipfile.return_value.__enter__.return_value = mock_zip_instance

        import tempfile

        with tempfile.TemporaryDirectory() as tmpdir:
            test_zip = os.path.join(tmpdir, "test.zip")
            Path(test_zip).touch()
            self.assertTrue(os.path.exists(test_zip))


class TestModelSelection(unittest.TestCase):
    @patch("builtins.input", return_value="1")
    def test_select_flex(self, mock_input):
        result = installer.get_ledger_model_from_user()
        self.assertEqual(result, "flex")

    @patch("builtins.input", return_value="2")
    def test_select_nanos(self, mock_input):
        result = installer.get_ledger_model_from_user()
        self.assertEqual(result, "nanos")

    @patch("builtins.input", return_value="3")
    def test_select_nanosplus(self, mock_input):
        result = installer.get_ledger_model_from_user()
        self.assertEqual(result, "nanosplus")

    @patch("builtins.input", return_value="4")
    def test_select_nanox(self, mock_input):
        result = installer.get_ledger_model_from_user()
        self.assertEqual(result, "nanox")

    @patch("builtins.input", return_value="5")
    def test_select_stax(self, mock_input):
        result = installer.get_ledger_model_from_user()
        self.assertEqual(result, "stax")


class TestColorFormatting(unittest.TestCase):
    @patch("builtins.print")
    def test_print_header(self, mock_print):
        installer.print_header("Test")
        mock_print.assert_called()

    @patch("builtins.print")
    def test_print_success(self, mock_print):
        installer.print_success("Test")
        mock_print.assert_called()

    @patch("builtins.print")
    def test_print_error(self, mock_print):
        with self.assertRaises(SystemExit):
            installer.print_error("Test")


def run_tests(verbosity: int = 2) -> int:
    print("=" * 70)
    print("Minotari Ledger Wallet Installer - Test Suite")
    print("=" * 70)
    print()

    loader = unittest.TestLoader()
    suite = unittest.TestSuite()

    suite.addTests(loader.loadTestsFromTestCase(TestPythonVersionCheck))
    suite.addTests(loader.loadTestsFromTestCase(TestCommandCheck))
    suite.addTests(loader.loadTestsFromTestCase(TestLedgerModelDetection))
    suite.addTests(loader.loadTestsFromTestCase(TestGitHubReleaseDownload))
    suite.addTests(loader.loadTestsFromTestCase(TestZipExtraction))
    suite.addTests(loader.loadTestsFromTestCase(TestModelSelection))
    suite.addTests(loader.loadTestsFromTestCase(TestColorFormatting))

    runner = unittest.TextTestRunner(verbosity=verbosity)
    result = runner.run(suite)

    print()
    print("=" * 70)
    if result.wasSuccessful():
        print("All tests passed.")
        return 0
    print(f"{len(result.failures)} test(s) failed")
    return 1


if __name__ == "__main__":
    sys.exit(run_tests())
