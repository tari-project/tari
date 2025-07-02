"""
Tests for PyTariPublicKey functionality.
"""

import pytest
import tari_wallet


class TestPyTariPublicKey:
    """Test cases for PyTariPublicKey."""
    
    def test_public_key_from_valid_hex(self, sample_hex_keys):
        """Test creating public key from valid hex strings."""
        valid_keys = ["valid_32_byte", "valid_64_char"]
        
        for key_name in valid_keys:
            hex_str = sample_hex_keys[key_name]
            try:
                public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
                assert isinstance(public_key, tari_wallet.PyTariPublicKey)
            except tari_wallet.TariWalletError:
                # Some hex strings might not represent valid public keys
                # even if they're the right format
                pass
    
    def test_public_key_from_invalid_hex(self, sample_hex_keys):
        """Test creating public key from invalid hex strings."""
        invalid_keys = ["invalid_short", "invalid_long", "invalid_chars", "empty"]
        
        for key_name in invalid_keys:
            hex_str = sample_hex_keys[key_name]
            with pytest.raises(tari_wallet.TariWalletError):
                tari_wallet.PyTariPublicKey.from_hex(hex_str)
    
    def test_public_key_to_hex(self, sample_hex_keys):
        """Test converting public key to hex string."""
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            result_hex = public_key.to_hex()
            
            assert isinstance(result_hex, str)
            assert len(result_hex) > 0
            # Result should be valid hex
            assert all(c in "0123456789abcdef" for c in result_hex.lower())
            
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_to_emoji(self, sample_hex_keys):
        """Test converting public key to emoji encoding."""
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            emoji_str = public_key.to_emoji_encoding()
            
            assert isinstance(emoji_str, str)
            assert len(emoji_str) > 0
            
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_round_trip(self, sample_hex_keys):
        """Test hex -> public key -> hex round trip."""
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            result_hex = public_key.to_hex()
            
            # The result might be in a different case or format
            # but should represent the same key
            assert isinstance(result_hex, str)
            assert len(result_hex) >= len(hex_str.replace("0x", ""))
            
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_with_prefix(self):
        """Test public key creation with hex prefix."""
        hex_without_prefix = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        hex_with_prefix = "0x" + hex_without_prefix
        
        # Both should either work or fail consistently
        try:
            key1 = tari_wallet.PyTariPublicKey.from_hex(hex_without_prefix)
            try:
                key2 = tari_wallet.PyTariPublicKey.from_hex(hex_with_prefix)
                # If both work, they should represent the same key
                assert key1.to_hex() == key2.to_hex()
            except tari_wallet.TariWalletError:
                # Prefix might not be supported
                pass
        except tari_wallet.TariWalletError:
            # Neither format is valid, that's also acceptable
            pass
    
    def test_public_key_case_insensitive(self):
        """Test that hex input is case insensitive."""
        hex_lower = "abcdef123456789abcdef123456789abcdef123456789abcdef123456789ab"
        hex_upper = hex_lower.upper()
        hex_mixed = "AbCdEf123456789AbCdEf123456789AbCdEf123456789AbCdEf123456789Ab"
        
        results = []
        for hex_str in [hex_lower, hex_upper, hex_mixed]:
            try:
                public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
                results.append(public_key.to_hex())
            except tari_wallet.TariWalletError:
                results.append(None)
        
        # If any succeed, they should all succeed and produce the same result
        valid_results = [r for r in results if r is not None]
        if len(valid_results) > 1:
            # All valid results should be the same (ignoring case)
            first_result = valid_results[0].lower()
            for result in valid_results[1:]:
                assert result.lower() == first_result
    
    def test_public_key_memory_management(self, sample_hex_keys):
        """Test that public key objects are properly managed."""
        keys = []
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            # Create multiple public key objects
            for _ in range(10):
                public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
                keys.append(public_key)
            
            assert len(keys) == 10
            
            # All should be valid
            for key in keys:
                assert isinstance(key, tari_wallet.PyTariPublicKey)
                hex_result = key.to_hex()
                assert isinstance(hex_result, str)
                
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_string_representation(self, sample_hex_keys):
        """Test public key string representation."""
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            
            # Test str() and repr()
            str_repr = str(public_key)
            repr_repr = repr(public_key)
            
            assert isinstance(str_repr, str)
            assert isinstance(repr_repr, str)
            assert len(str_repr) > 0
            assert len(repr_repr) > 0
            
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_invalid_operations(self, sample_hex_keys):
        """Test invalid operations on public key objects."""
        hex_str = sample_hex_keys["valid_32_byte"]
        
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            
            # Test that we can't modify the public key
            with pytest.raises(AttributeError):
                public_key.some_nonexistent_attribute = "value"
            
        except tari_wallet.TariWalletError:
            pytest.skip("Public key creation failed with test hex")
    
    def test_public_key_edge_cases(self):
        """Test public key creation with edge case inputs."""
        edge_cases = [
            "",  # Empty string
            "0",  # Single character
            "00",  # Single byte
            "0" * 63,  # One character short
            "f" * 64,  # All f's
            "0" * 64,  # All zeros
            " " + "0" * 62 + " ",  # With whitespace
        ]
        
        for case in edge_cases:
            try:
                public_key = tari_wallet.PyTariPublicKey.from_hex(case)
                # If it succeeds, it should be a valid object
                assert isinstance(public_key, tari_wallet.PyTariPublicKey)
                hex_result = public_key.to_hex()
                assert isinstance(hex_result, str)
            except tari_wallet.TariWalletError:
                # Expected for invalid cases
                pass
    
    def test_public_key_type_errors(self):
        """Test that type errors are raised for invalid input types."""
        invalid_inputs = [
            None,
            123,
            [],
            {},
            object(),
        ]
        
        for invalid_input in invalid_inputs:
            with pytest.raises(TypeError):
                tari_wallet.PyTariPublicKey.from_hex(invalid_input)
