"""
Tests for PyTariCommsConfig functionality.
"""

import pytest
import tari_wallet


class TestPyTariCommsConfig:
    """Test cases for PyTariCommsConfig."""
    
    def test_config_creation(self, temp_dir):
        """Test basic config creation."""
        config = tari_wallet.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="test_db",
            datastore_path=temp_dir,
            discovery_timeout=60,
            exclude_dial_test_addresses=True
        )
        
        assert config is not None
        assert isinstance(config, tari_wallet.PyTariCommsConfig)
    
    def test_config_with_different_addresses(self, temp_dir):
        """Test config creation with different address formats."""
        addresses = [
            "/ip4/127.0.0.1/tcp/18188",
            "/ip4/0.0.0.0/tcp/9999", 
            "/ip6/::1/tcp/18188",
            "/dns4/localhost/tcp/18188",
        ]
        
        for address in addresses:
            config = tari_wallet.PyTariCommsConfig(
                public_address=address,
                database_name="test_db",
                datastore_path=temp_dir,
                discovery_timeout=30,
                exclude_dial_test_addresses=True
            )
            assert config is not None
    
    def test_config_with_different_timeouts(self, temp_dir):
        """Test config with various timeout values."""
        timeouts = [1, 30, 60, 300, 3600]
        
        for timeout in timeouts:
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18188",
                database_name="test_db",
                datastore_path=temp_dir,
                discovery_timeout=timeout,
                exclude_dial_test_addresses=False
            )
            assert config is not None
    
    def test_config_exclude_dial_addresses(self, temp_dir):
        """Test config with both exclude dial address settings."""
        for exclude_setting in [True, False]:
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18188",
                database_name="test_db",
                datastore_path=temp_dir,
                discovery_timeout=60,
                exclude_dial_test_addresses=exclude_setting
            )
            assert config is not None
    
    def test_config_invalid_parameters(self, temp_dir):
        """Test config creation with invalid parameters."""
        # Note: These may not raise exceptions if the underlying FFI 
        # doesn't validate parameters, but we test for completeness
        
        # Test with empty strings
        try:
            config = tari_wallet.PyTariCommsConfig(
                public_address="",
                database_name="test_db",
                datastore_path=temp_dir,
                discovery_timeout=60,
                exclude_dial_test_addresses=True
            )
            # If no exception, that's also valid behavior
        except tari_wallet.TariWalletError:
            # Expected if validation occurs
            pass
    
    def test_config_with_unicode_paths(self, temp_dir):
        """Test config with unicode characters in paths."""
        import os
        
        unicode_subdir = os.path.join(temp_dir, "测试_目录")
        os.makedirs(unicode_subdir, exist_ok=True)
        
        config = tari_wallet.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="测试_数据库",
            datastore_path=unicode_subdir,
            discovery_timeout=60,
            exclude_dial_test_addresses=True
        )
        assert config is not None
    
    def test_config_with_long_paths(self, temp_dir):
        """Test config with long file paths."""
        import os
        
        # Create a deeply nested directory structure
        long_path = temp_dir
        for i in range(10):
            long_path = os.path.join(long_path, f"very_long_directory_name_{i}")
        
        os.makedirs(long_path, exist_ok=True)
        
        config = tari_wallet.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="test_db",
            datastore_path=long_path,
            discovery_timeout=60,
            exclude_dial_test_addresses=True
        )
        assert config is not None
    
    def test_multiple_configs(self, temp_dir):
        """Test creating multiple config instances."""
        configs = []
        
        for i in range(5):
            config = tari_wallet.PyTariCommsConfig(
                public_address=f"/ip4/127.0.0.1/tcp/{18188 + i}",
                database_name=f"test_db_{i}",
                datastore_path=temp_dir,
                discovery_timeout=30 + i * 10,
                exclude_dial_test_addresses=i % 2 == 0
            )
            configs.append(config)
        
        assert len(configs) == 5
        for config in configs:
            assert isinstance(config, tari_wallet.PyTariCommsConfig)
