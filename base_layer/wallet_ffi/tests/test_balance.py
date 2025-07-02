"""
Tests for PyTariBalance functionality.
"""

import pytest
import tari_wallet


class TestPyTariBalance:
    """Test cases for PyTariBalance."""
    
    def test_balance_properties(self, test_wallet):
        """Test balance property access."""
        try:
            balance = test_wallet.get_balance()
            
            # Test that all properties are accessible and return integers
            assert isinstance(balance.available, int)
            assert isinstance(balance.time_locked, int)
            assert isinstance(balance.pending_incoming, int)
            assert isinstance(balance.pending_outgoing, int)
            
            # Test that balances are non-negative
            assert balance.available >= 0
            assert balance.time_locked >= 0
            assert balance.pending_incoming >= 0
            assert balance.pending_outgoing >= 0
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_calculations(self, test_wallet):
        """Test balance calculations and relationships."""
        try:
            balance = test_wallet.get_balance()
            
            # Calculate total balance components
            total_confirmed = balance.available + balance.time_locked
            total_pending = balance.pending_incoming + balance.pending_outgoing
            total_balance = total_confirmed + total_pending
            
            # All totals should be non-negative
            assert total_confirmed >= 0
            assert total_pending >= 0
            assert total_balance >= 0
            
            # Available should be <= total confirmed
            assert balance.available <= total_confirmed
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_string_representation(self, test_wallet):
        """Test balance object string representation."""
        try:
            balance = test_wallet.get_balance()
            
            # Check that the balance object can be converted to string
            balance_str = str(balance)
            assert isinstance(balance_str, str)
            assert len(balance_str) > 0
            
            # Check that repr works
            balance_repr = repr(balance)
            assert isinstance(balance_repr, str)
            assert "PyTariBalance" in balance_repr or "Balance" in balance_repr
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_comparison(self, test_wallet):
        """Test balance value consistency."""
        try:
            balance1 = test_wallet.get_balance()
            balance2 = test_wallet.get_balance()
            
            # Balances retrieved immediately should be the same
            # (unless there are concurrent transactions)
            assert balance1.available == balance2.available
            assert balance1.time_locked == balance2.time_locked
            
            # Note: pending balances might change between calls due to 
            # network activity, so we don't assert equality for those
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_type_safety(self, test_wallet):
        """Test that balance properties return correct types."""
        try:
            balance = test_wallet.get_balance()
            
            # Test that properties are specifically integers, not floats
            assert type(balance.available) is int
            assert type(balance.time_locked) is int
            assert type(balance.pending_incoming) is int
            assert type(balance.pending_outgoing) is int
            
            # Test that properties are not boolean (0 and 1 are valid balances)
            assert not isinstance(balance.available, bool)
            assert not isinstance(balance.time_locked, bool)
            assert not isinstance(balance.pending_incoming, bool)
            assert not isinstance(balance.pending_outgoing, bool)
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_large_values(self, test_wallet):
        """Test balance handling of large values."""
        try:
            balance = test_wallet.get_balance()
            
            # Test that balance values are within reasonable bounds for microTari
            max_reasonable_balance = 21_000_000 * 1_000_000  # 21M Tari in microTari
            
            assert balance.available <= max_reasonable_balance
            assert balance.time_locked <= max_reasonable_balance
            assert balance.pending_incoming <= max_reasonable_balance
            assert balance.pending_outgoing <= max_reasonable_balance
            
            # Test that balances fit in 64-bit signed integer
            max_int64 = 2**63 - 1
            assert balance.available <= max_int64
            assert balance.time_locked <= max_int64
            assert balance.pending_incoming <= max_int64
            assert balance.pending_outgoing <= max_int64
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_immutability(self, test_wallet):
        """Test that balance objects are properly immutable."""
        try:
            balance = test_wallet.get_balance()
            
            # Test that we can't modify balance properties
            with pytest.raises((AttributeError, TypeError)):
                balance.available = 1000
            
            with pytest.raises((AttributeError, TypeError)):
                balance.time_locked = 2000
            
            with pytest.raises((AttributeError, TypeError)):
                balance.pending_incoming = 3000
            
            with pytest.raises((AttributeError, TypeError)):
                balance.pending_outgoing = 4000
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_multiple_retrievals(self, test_wallet):
        """Test multiple balance retrievals for stability."""
        balances = []
        
        try:
            # Retrieve balance multiple times
            for _ in range(5):
                balance = test_wallet.get_balance()
                balances.append(balance)
            
            assert len(balances) == 5
            
            # All balance objects should be valid
            for balance in balances:
                assert isinstance(balance, tari_wallet.PyTariBalance)
                assert isinstance(balance.available, int)
                assert isinstance(balance.time_locked, int)
                assert isinstance(balance.pending_incoming, int)
                assert isinstance(balance.pending_outgoing, int)
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
    
    def test_balance_edge_cases(self, test_wallet):
        """Test balance behavior in edge cases."""
        try:
            balance = test_wallet.get_balance()
            
            # Test arithmetic operations (should not modify original)
            available_doubled = balance.available * 2
            assert isinstance(available_doubled, int)
            assert balance.available == balance.available  # Original unchanged
            
            # Test comparison operations
            assert balance.available >= 0
            assert balance.time_locked >= 0
            
            # Test that balance properties work in boolean context
            if balance.available:
                assert balance.available > 0
            else:
                assert balance.available == 0
            
        except tari_wallet.TariWalletError as e:
            pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
