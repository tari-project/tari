#!/usr/bin/env python3
"""
Unit tests for the unified Ledger installer.

Run with: python3 -m pytest test_installer.py -v
Or: python3 test_installer.py
"""

import unittest
from unittest.mock import patch, MagicMock, mock_open
import sys
import os
import json
import tempfile
import zipfile
from pathlib import Path

# Add parent directory to path to import the installer
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import install_minotari_ledger as installer


class TestPythonVersionCheck(unittest.TestCase):
    """Test Python version checking."""
    
    def test_python_3_8_passes(self):
        """Python 3.8+ should pass."""
        # Create a mock version_info with proper attributes
        mock_version = MagicMock()
        mock_version.major = 3
        mock_version.minor = 8
        mock_version.__lt__ = lambda self, other: (3, 8) < other
        
        with patch('sys.version_info', mock_version):
            self.assertTrue(installer.check_python_version())
    
    def test_python_3_10_passes(self):
        """Python 3.10 should pass."""
        mock_version = MagicMock()
        mock_version.major = 3
        mock_version.minor = 10
        mock_version.__lt__ = lambda self, other: (3, 10) < other
        
        with patch('sys.version_info', mock_version):
            self.assertTrue(installer.check_python_version())
    
    def test_python_3_7_fails(self):
        """Python 3.7 should fail."""
        mock_version = MagicMock()
        mock_version.major = 3
        mock_version.minor = 7
        mock_version.__lt__ = lambda self, other: (3, 7) < other
        
        with patch('sys.version_info', mock_version):
            self.assertFalse(installer.check_python_version())
    
    def test_python_2_fails(self):
        """Python 2 should fail."""
        mock_version = MagicMock()
        mock_version.major = 2
        mock_version.minor = 7
        mock_version.__lt__ = lambda self, other: (2, 7) < other
        
        with patch('sys.version_info', mock_version):
            self.assertFalse(installer.check_python_version())


class TestLedgerProductIds(unittest.TestCase):
    """Test Ledger product ID mappings."""
    
    def test_nanosplus_product_id(self):
        """Nano S Plus should have correct product ID."""
        self.assertEqual(installer.LEDGER_PRODUCT_IDS[0x0005], "nanosplus")
    
    def test_nanox_product_id(self):
        """Nano X should have correct product ID."""
        self.assertEqual(installer.LEDGER_PRODUCT_IDS[0x0004], "nanox")
    
    def test_stax_product_id(self):
        """Stax should have correct product ID."""
        self.assertEqual(installer.LEDGER_PRODUCT_IDS[0x0006], "stax")
    
    def test_flex_product_id(self):
        """Flex should have correct product ID."""
        self.assertEqual(installer.LEDGER_PRODUCT_IDS[0x0007], "flex")
    
    def test_vendor_id(self):
        """Ledger vendor ID should be correct."""
        self.assertEqual(installer.LEDGER_VID, 0x2C97)


class TestModelNames(unittest.TestCase):
    """Test model name mappings."""
    
    def test_model_names_exist(self):
        """All product IDs should have display names."""
        for model in installer.LEDGER_PRODUCT_IDS.values():
            self.assertIn(model, installer.MODEL_NAMES)
    
    def test_nanosplus_name(self):
        """Nano S Plus display name should be correct."""
        self.assertEqual(installer.MODEL_NAMES["nanosplus"], "Nano S Plus")


class TestAssetFinding(unittest.TestCase):
    """Test finding correct assets for models."""
    
    def setUp(self):
        """Set up test release data."""
        self.release_data = {
            "tag_name": "v5.3.0",
            "assets": [
                {
                    "name": "minotari_ledger_wallet-nanosplus-v5.3.0.zip",
                    "browser_download_url": "https://example.com/nanosplus.zip"
                },
                {
                    "name": "minotari_ledger_wallet-nanox-v5.3.0.zip",
                    "browser_download_url": "https://example.com/nanox.zip"
                },
                {
                    "name": "minotari_ledger_wallet-flex-v5.3.0.zip",
                    "browser_download_url": "https://example.com/flex.zip"
                },
                {
                    "name": "minotari_ledger_wallet-stax-v5.3.0.zip",
                    "browser_download_url": "https://example.com/stax.zip"
                },
                {
                    "name": "minotari_ledger_wallet-nanosplus-v5.3.0.zip.sha256",
                    "browser_download_url": "https://example.com/nanosplus.zip.sha256"
                },
                {
                    "name": "other-file.zip",
                    "browser_download_url": "https://example.com/other.zip"
                }
            ]
        }
    
    def test_find_nanosplus_asset(self):
        """Should find Nano S Plus asset."""
        asset = installer.find_asset_for_model(self.release_data, "nanosplus")
        self.assertIsNotNone(asset)
        self.assertIn("nanosplus", asset["name"])
        self.assertTrue(asset["name"].endswith(".zip"))
        self.assertFalse(asset["name"].endswith(".sha256"))
    
    def test_find_nanox_asset(self):
        """Should find Nano X asset."""
        asset = installer.find_asset_for_model(self.release_data, "nanox")
        self.assertIsNotNone(asset)
        self.assertIn("nanox", asset["name"])
    
    def test_find_flex_asset(self):
        """Should find Flex asset."""
        asset = installer.find_asset_for_model(self.release_data, "flex")
        self.assertIsNotNone(asset)
        self.assertIn("flex", asset["name"])
    
    def test_find_stax_asset(self):
        """Should find Stax asset."""
        asset = installer.find_asset_for_model(self.release_data, "stax")
        self.assertIsNotNone(asset)
        self.assertIn("stax", asset["name"])
    
    def test_no_asset_for_unsupported_model(self):
        """Should return None for unsupported model."""
        asset = installer.find_asset_for_model(self.release_data, "unsupported")
        self.assertIsNone(asset)


class TestLedgerctlDetection(unittest.TestCase):
    """Test ledgerctl-based device detection."""
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_detect_nanosplus(self, mock_run):
        """Should detect Nano S Plus from ledgerctl output."""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Device: Nano S Plus\nVersion: 2.1.0"
        )
        
        result = installer.detect_ledger_ledgerctl()
        self.assertEqual(result, "nanosplus")
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_detect_nanox(self, mock_run):
        """Should detect Nano X from ledgerctl output."""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Device: Nano X\nVersion: 2.0.2"
        )
        
        result = installer.detect_ledger_ledgerctl()
        self.assertEqual(result, "nanox")
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_detect_stax(self, mock_run):
        """Should detect Stax from ledgerctl output."""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Device: Stax\nVersion: 1.0.0"
        )
        
        result = installer.detect_ledger_ledgerctl()
        self.assertEqual(result, "stax")
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_detect_flex(self, mock_run):
        """Should detect Flex from ledgerctl output."""
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Device: Flex\nVersion: 1.0.0"
        )
        
        result = installer.detect_ledger_ledgerctl()
        self.assertEqual(result, "flex")
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_no_device_connected(self, mock_run):
        """Should return None when no device connected."""
        mock_run.return_value = MagicMock(
            returncode=1,
            stderr="No device found"
        )
        
        result = installer.detect_ledger_ledgerctl()
        self.assertIsNone(result)
    
    @patch('install_minotari_ledger.subprocess.run')
    def test_nanosplus_not_confused_with_nanos(self, mock_run):
        """Should not confuse Nano S Plus with Nano S."""
        # This tests the bug fix where "nano s" is a substring of "nano s plus"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout="Device: Nano S Plus\nVersion: 2.1.0"
        )
        
        result = installer.detect_ledger_ledgerctl()
        # Should return nanosplus, not nanos
        self.assertEqual(result, "nanosplus")


class TestHidDetection(unittest.TestCase):
    """Test HID-based device detection."""
    
    @patch.dict('sys.modules', {'hid': MagicMock()})
    def test_detect_nanosplus_via_hid(self):
        """Should detect Nano S Plus via HID."""
        import sys
        mock_hid = sys.modules['hid']
        mock_hid.enumerate.return_value = [
            {"vendor_id": 0x2C97, "product_id": 0x0005}
        ]
        
        result = installer.detect_ledger_hid()
        self.assertEqual(result, "nanosplus")
    
    @patch.dict('sys.modules', {'hid': MagicMock()})
    def test_detect_nanox_via_hid(self):
        """Should detect Nano X via HID."""
        import sys
        mock_hid = sys.modules['hid']
        mock_hid.enumerate.return_value = [
            {"vendor_id": 0x2C97, "product_id": 0x0004}
        ]
        
        result = installer.detect_ledger_hid()
        self.assertEqual(result, "nanox")
    
    @patch.dict('sys.modules', {'hid': MagicMock()})
    def test_no_ledger_connected(self):
        """Should return None when no Ledger connected."""
        import sys
        mock_hid = sys.modules['hid']
        mock_hid.enumerate.return_value = [
            {"vendor_id": 0x1234, "product_id": 0x5678}  # Some other device
        ]
        
        result = installer.detect_ledger_hid()
        self.assertIsNone(result)
    
    @patch.dict('sys.modules', {'hid': None})
    def test_hid_import_error(self):
        """Should handle missing hid module gracefully."""
        result = installer.detect_ledger_hid()
        self.assertIsNone(result)


class TestFirmwareExtraction(unittest.TestCase):
    """Test firmware zip extraction."""
    
    def setUp(self):
        """Create a temporary directory for tests."""
        self.temp_dir = tempfile.mkdtemp()
    
    def tearDown(self):
        """Clean up temporary directory."""
        import shutil
        shutil.rmtree(self.temp_dir, ignore_errors=True)
    
    def test_extract_app_json(self):
        """Should extract and find app_*.json file."""
        # Create a test zip file
        zip_path = os.path.join(self.temp_dir, "test.zip")
        extract_dir = os.path.join(self.temp_dir, "extracted")
        
        with zipfile.ZipFile(zip_path, 'w') as zf:
            zf.writestr("app_nanosplus.json", '{"name": "Minotari"}')
            zf.writestr("README.txt", "Some readme")
        
        result = installer.extract_firmware(zip_path, extract_dir)
        
        self.assertIsNotNone(result)
        self.assertTrue(result.endswith("app_nanosplus.json"))
        self.assertTrue(os.path.exists(result))
    
    def test_extract_no_app_json(self):
        """Should return None if no app_*.json found."""
        zip_path = os.path.join(self.temp_dir, "test.zip")
        extract_dir = os.path.join(self.temp_dir, "extracted")
        
        with zipfile.ZipFile(zip_path, 'w') as zf:
            zf.writestr("README.txt", "Some readme")
            zf.writestr("firmware.bin", "binary data")
        
        result = installer.extract_firmware(zip_path, extract_dir)
        
        self.assertIsNone(result)
    
    def test_extract_invalid_zip(self):
        """Should handle invalid zip file."""
        zip_path = os.path.join(self.temp_dir, "not_a_zip.zip")
        extract_dir = os.path.join(self.temp_dir, "extracted")
        
        # Create a file that's not a zip
        with open(zip_path, 'w') as f:
            f.write("This is not a zip file")
        
        result = installer.extract_firmware(zip_path, extract_dir)
        
        self.assertIsNone(result)


class TestGitHubReleaseFetching(unittest.TestCase):
    """Test GitHub release fetching."""
    
    @patch('install_minotari_ledger.urlopen')
    def test_fetch_release_success(self, mock_urlopen):
        """Should successfully fetch release data."""
        mock_response = MagicMock()
        mock_response.read.return_value = json.dumps({
            "tag_name": "v5.3.0",
            "assets": [{"name": "test.zip"}]
        }).encode()
        mock_urlopen.return_value.__enter__.return_value = mock_response
        
        result = installer.fetch_latest_release()
        
        self.assertEqual(result["tag_name"], "v5.3.0")
    
    @patch('install_minotari_ledger.urlopen')
    def test_fetch_release_http_error(self, mock_urlopen):
        """Should handle HTTP errors."""
        from urllib.error import HTTPError
        mock_urlopen.side_effect = HTTPError(
            url="https://api.github.com/repos/tari-project/tari/releases/latest",
            code=404,
            msg="Not Found",
            hdrs={},
            fp=None
        )
        
        with self.assertRaises(HTTPError):
            installer.fetch_latest_release()


class TestNamingConventions(unittest.TestCase):
    """Test that naming conventions match GitHub assets."""
    
    def test_model_slug_matches_asset_pattern(self):
        """
        Verify model slugs match the actual GitHub asset naming.
        
        Asset naming: minotari_ledger_wallet-{model}-v{version}-{hash}.zip
        Example: minotari_ledger_wallet-nanosplus-v5.3.0-rc.1-e8fc0e4.zip
        
        The model slug should NOT be "nanos+" - it should be "nanosplus"
        """
        # This test documents the correct naming convention
        # PR #7805 had a bug where they used "nanos+" instead of "nanosplus"
        supported_models = ["nanosplus", "nanox", "stax", "flex"]
        
        for model in supported_models:
            # Simulate asset name
            asset_name = f"minotari_ledger_wallet-{model}-v5.3.0.zip"
            
            # Verify the pattern matches what we expect
            self.assertTrue(
                asset_name.startswith(f"minotari_ledger_wallet-{model}-"),
                f"Model slug '{model}' should produce valid asset name"
            )
    
    def test_nanosplus_not_nanos_plus(self):
        """
        Critical test: Nano S Plus model should use 'nanosplus', not 'nanos+'.
        
        This was a bug in PR #7805 where they changed the slug to 'nanos+'
        which caused 404 errors when downloading assets.
        """
        self.assertIn("nanosplus", installer.MODEL_NAMES)
        self.assertNotIn("nanos+", installer.MODEL_NAMES)
        
        # Verify the product ID mapping
        self.assertEqual(installer.LEDGER_PRODUCT_IDS[0x0005], "nanosplus")


class TestIntegration(unittest.TestCase):
    """Integration tests that verify end-to-end workflows."""
    
    @patch('install_minotari_ledger.detect_ledger_hid')
    @patch('install_minotari_ledger.detect_ledger_ledgerctl')
    def test_detection_fallback(self, mock_ledgerctl, mock_hid):
        """Should fallback to ledgerctl when HID fails."""
        mock_hid.return_value = None
        mock_ledgerctl.return_value = "nanox"
        
        result = installer.detect_ledger_model()
        
        self.assertEqual(result, "nanox")
        mock_hid.assert_called_once()
        mock_ledgerctl.assert_called_once()


if __name__ == "__main__":
    # Run tests with verbose output
    unittest.main(verbosity=2)
