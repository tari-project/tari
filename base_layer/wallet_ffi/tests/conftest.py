"""
Pytest configuration and fixtures for Tari wallet Python bindings tests.
"""

import pytest
import tempfile
import shutil
import os
import tari_wallet
from fixtures.mock_callbacks import MockCallbackRegistry, mock_callback_registry


@pytest.fixture
def temp_dir():
    """Create a temporary directory for test data."""
    temp_path = tempfile.mkdtemp(prefix="tari_test_")
    yield temp_path
    shutil.rmtree(temp_path, ignore_errors=True)


@pytest.fixture
def basic_config(temp_dir):
    """Create a basic wallet configuration for testing."""
    transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
    return tari_wallet.PyTariCommsConfig(
        public_address="/ip4/127.0.0.1/tcp/18188",
        database_name="test_wallet",
        datastore_path=temp_dir,
        discovery_timeout=30,
        exclude_dial_test_addresses=True,
        transport=transport
    )


@pytest.fixture
def test_wallet(basic_config, temp_dir):
    """Create a test wallet instance."""
    return tari_wallet.PyTariWallet(
        config=basic_config,
        log_path=os.path.join(temp_dir, "logs"),
        log_verbosity=1,
        num_rolling_log_files=3,
        size_per_log_file_bytes=512*1024,
        network_str="nextnet",
        passphrase="test_wallet_passphrase"
    )


@pytest.fixture
def event_collector():
    """Create an event collector for testing callbacks."""
    class EventCollector:
        def __init__(self):
            self.events = []
        
        def collect_event(self, event_name):
            def handler(*args):
                self.events.append((event_name, args))
            return handler
        
        def get_events(self, event_name=None):
            if event_name:
                return [args for name, args in self.events if name == event_name]
            return self.events
        
        def clear_events(self):
            self.events.clear()
    
    return EventCollector()


@pytest.fixture
def wallet_with_callbacks(basic_config, temp_dir, event_collector):
    """Create a wallet with event callbacks for testing."""
    callbacks = {
        "balance_updated": event_collector.collect_event("balance_updated"),
        "transaction_mined": event_collector.collect_event("transaction_mined"),
        "connectivity_status": event_collector.collect_event("connectivity_status"),
    }
    
    return tari_wallet.PyTariWallet(
        config=basic_config,
        log_path=os.path.join(temp_dir, "logs"),
        log_verbosity=2,  # Debug level for tests
        num_rolling_log_files=3,
        size_per_log_file_bytes=256*1024,
        network_str="nextnet",
        passphrase="test_wallet_passphrase",
        callbacks=callbacks
    )


@pytest.fixture
def sample_hex_keys():
    """Provide sample hex keys for testing."""
    return {
        "valid_32_byte": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "valid_64_char": "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210",
        "invalid_short": "0123456789abcdef",
        "invalid_long": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef00",
        "invalid_chars": "0123456789abcdefg123456789abcdef0123456789abcdef0123456789abcdef",
        "empty": "",
    }


@pytest.fixture
def test_messages():
    """Provide test messages for signing/verification."""
    return [
        "Hello, Tari!",
        "Test message with numbers 123",
        "Unicode test: 你好 🌟",
        "",  # Empty message
        "A" * 1000,  # Long message
        "Special chars: !@#$%^&*()_+-=[]{}|;:,.<>?",
    ]


@pytest.fixture
def mock_callbacks():
    """Provide mock callback registry for testing."""
    # Clear any previous state
    mock_callback_registry.clear_stats()
    yield mock_callback_registry
    # Clean up after test
    mock_callback_registry.clear_stats()


@pytest.fixture
def callback_tracker():
    """Provide a fresh callback tracker for each test."""
    from fixtures.mock_callbacks import MockCallbackRegistry
    tracker = MockCallbackRegistry()
    yield tracker
    tracker.clear_stats()
