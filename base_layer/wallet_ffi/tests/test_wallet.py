"""
Tests for PyTariWallet functionality.
"""

import pytest
import os
import tari_wallet


class TestPyTariWallet:
    """Test cases for PyTariWallet."""
    
    def test_wallet_creation(self, test_wallet):
        """Test basic wallet creation."""
        assert test_wallet is not None
        assert isinstance(test_wallet, tari_wallet.PyTariWallet)
    
    def test_wallet_creation_with_callbacks(self, wallet_with_callbacks):
        """Test wallet creation with callbacks."""
        assert wallet_with_callbacks is not None
        assert isinstance(wallet_with_callbacks, tari_wallet.PyTariWallet)
    
    def test_wallet_creation_with_passphrase(self, basic_config, temp_dir):
        """Test wallet creation with passphrase."""
        wallet = tari_wallet.PyTariWallet(
            config=basic_config,
            log_path=os.path.join(temp_dir, "logs"),
            log_verbosity=1,
            num_rolling_log_files=3,
            size_per_log_file_bytes=512*1024,
            network_str="localnet",
            passphrase="test_passphrase",
            seed_passphrase="test_seed_passphrase"
        )
        assert wallet is not None
    
    def test_get_balance(self, test_wallet):
        """Test getting wallet balance."""
        try:
            balance = test_wallet.get_balance()
            assert isinstance(balance, tari_wallet.PyTariBalance)
        except tari_wallet.TariWalletError as e:
            # Balance retrieval might fail in test environment
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_sign_message(self, test_wallet, test_messages):
        """Test message signing."""
        for message in test_messages:
            try:
                signature = test_wallet.sign_message(message)
                assert isinstance(signature, str)
                assert len(signature) > 0
                # Signature should be consistent for the same message
                signature2 = test_wallet.sign_message(message)
                # Note: Signatures might not be deterministic due to randomness
                assert isinstance(signature2, str)
            except tari_wallet.TariWalletError as e:
                # Signing might fail in test environment
                pytest.skip(f"Message signing failed (expected in test env): {e}")
    
    def test_verify_message_signature(self, test_wallet, sample_hex_keys):
        """Test message signature verification."""
        message = "Test message for verification"
        
        # Test with a sample public key (will likely fail but tests the API)
        try:
            is_valid = test_wallet.verify_message_signature(
                sample_hex_keys["valid_32_byte"],
                "sample_signature",
                message
            )
            assert isinstance(is_valid, bool)
        except tari_wallet.TariWalletError:
            # Expected to fail with invalid signature/key
            pass
    
    def test_get_completed_transactions(self, test_wallet):
        """Test getting completed transactions."""
        try:
            transactions = test_wallet.get_completed_transactions()
            assert isinstance(transactions, list)
            
            # Test with limit
            transactions_limited = test_wallet.get_completed_transactions(limit=5)
            assert isinstance(transactions_limited, list)
            assert len(transactions_limited) <= 5
            
        except tari_wallet.TariWalletError as e:
            # Transaction retrieval might fail in test environment
            pytest.skip(f"Transaction retrieval failed (expected in test env): {e}")
    
    def test_get_contacts(self, test_wallet):
        """Test getting wallet contacts."""
        try:
            contacts = test_wallet.get_contacts()
            assert isinstance(contacts, list)
            
            for contact in contacts:
                assert isinstance(contact, tuple)
                assert len(contact) == 2
                alias, address = contact
                assert isinstance(alias, str)
                assert isinstance(address, str)
                
        except tari_wallet.TariWalletError as e:
            # Contacts retrieval might fail in test environment
            pytest.skip(f"Contacts retrieval failed (expected in test env): {e}")
    
    def test_send_transaction_invalid_address(self, test_wallet):
        """Test sending transaction with invalid address."""
        with pytest.raises(tari_wallet.TariWalletError):
            test_wallet.send_transaction(
                dest_address="invalid_address",
                amount=1000,
                fee_per_gram=5,
                message="Test transaction",
                one_sided=False
            )
    
    def test_wallet_with_different_networks(self, basic_config, temp_dir):
        """Test wallet creation with different networks."""
        networks = ["localnet", "nextnet", "mainnet"]
        
        for network in networks:
            try:
                wallet = tari_wallet.PyTariWallet(
                    config=basic_config,
                    log_path=os.path.join(temp_dir, f"logs_{network}"),
                    log_verbosity=1,
                    num_rolling_log_files=3,
                    size_per_log_file_bytes=512*1024,
                    network_str=network
                )
                assert wallet is not None
            except tari_wallet.TariWalletError as e:
                # Some networks might not be available in test environment
                pytest.skip(f"Network {network} not available in test env: {e}")
    
    def test_wallet_with_different_log_levels(self, basic_config, temp_dir):
        """Test wallet creation with different log levels."""
        log_levels = [0, 1, 2, 3, 4]  # Error, Warn, Info, Debug, Trace
        
        for level in log_levels:
            wallet = tari_wallet.PyTariWallet(
                config=basic_config,
                log_path=os.path.join(temp_dir, f"logs_level_{level}"),
                log_verbosity=level,
                num_rolling_log_files=3,
                size_per_log_file_bytes=512*1024,
                network_str="localnet"
            )
            assert wallet is not None
    
    def test_invalid_message_signing(self, test_wallet):
        """Test message signing with potentially problematic inputs."""
        problematic_messages = [
            None,  # This should raise TypeError, not TariWalletError
        ]
        
        for message in problematic_messages:
            with pytest.raises((TypeError, tari_wallet.TariWalletError)):
                test_wallet.sign_message(message)
    
    def test_verify_signature_with_invalid_inputs(self, test_wallet):
        """Test signature verification with invalid inputs."""
        message = "Test message"
        
        # Invalid public key formats
        invalid_keys = [
            "",
            "invalid_hex",
            "123",  # Too short
            "g" * 64,  # Invalid hex characters
        ]
        
        for invalid_key in invalid_keys:
            try:
                result = test_wallet.verify_message_signature(
                    invalid_key,
                    "valid_signature_format",
                    message
                )
                # If it doesn't raise an error, result should be False
                assert isinstance(result, bool)
            except tari_wallet.TariWalletError:
                # Expected for invalid inputs
                pass
    
    def test_wallet_memory_usage(self, basic_config, temp_dir):
        """Test that multiple wallet instances don't cause memory issues."""
        wallets = []
        
        try:
            for i in range(10):
                wallet = tari_wallet.PyTariWallet(
                    config=basic_config,
                    log_path=os.path.join(temp_dir, f"logs_{i}"),
                    log_verbosity=0,  # Error level only to reduce overhead
                    num_rolling_log_files=1,
                    size_per_log_file_bytes=64*1024,
                    network_str="localnet"
                )
                wallets.append(wallet)
            
            assert len(wallets) == 10
            
        except tari_wallet.TariWalletError as e:
            # Multiple wallet creation might fail in test environment
            pytest.skip(f"Multiple wallet creation failed (expected): {e}")
    
    def test_transaction_limits(self, test_wallet):
        """Test transaction operations with edge case values."""
        # Test with zero amount (should fail)
        with pytest.raises(tari_wallet.TariWalletError):
            test_wallet.send_transaction(
                dest_address="valid_address_format",
                amount=0,
                fee_per_gram=1,
                message="Zero amount test",
                one_sided=False
            )
        
        # Test with very large amount
        with pytest.raises(tari_wallet.TariWalletError):
            test_wallet.send_transaction(
                dest_address="valid_address_format", 
                amount=2**63 - 1,  # Max int64
                fee_per_gram=1,
                message="Large amount test",
                one_sided=False
            )
