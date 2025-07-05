#!/usr/bin/env python3
"""
Mock callback fixtures for testing Tari Wallet FFI callback functionality.

This module provides mock implementations of all wallet callbacks for testing
purposes, along with utilities for tracking and validating callback behavior.
"""

import time
import json
from typing import Dict, Any, List, Optional, Callable
from threading import Lock
from dataclasses import dataclass, field
from datetime import datetime


@dataclass
class CallbackInvocation:
    """Record of a single callback invocation."""
    callback_name: str
    timestamp: float
    args: tuple
    kwargs: dict
    processed: bool = False
    error: Optional[str] = None


@dataclass
class CallbackStats:
    """Statistics for a specific callback."""
    name: str
    total_invocations: int = 0
    last_invocation: Optional[float] = None
    average_interval: Optional[float] = None
    invocations: List[CallbackInvocation] = field(default_factory=list)


class MockCallbackRegistry:
    """Registry for managing mock callbacks and tracking their invocations."""
    
    def __init__(self):
        self.callbacks: Dict[str, Callable] = {}
        self.stats: Dict[str, CallbackStats] = {}
        self.invocations: List[CallbackInvocation] = []
        self.lock = Lock()
        self._setup_default_callbacks()
    
    def _setup_default_callbacks(self):
        """Set up default mock callbacks for all wallet callback types."""
        
        # Transaction callbacks
        self.register('received_transaction', self._mock_received_transaction)
        self.register('received_transaction_reply', self._mock_received_transaction_reply)
        self.register('received_finalized_transaction', self._mock_received_finalized_transaction)
        self.register('transaction_broadcast', self._mock_transaction_broadcast)
        self.register('transaction_mined', self._mock_transaction_mined)
        self.register('transaction_mined_unconfirmed', self._mock_transaction_mined_unconfirmed)
        self.register('faux_transaction_confirmed', self._mock_faux_transaction_confirmed)
        self.register('faux_transaction_unconfirmed', self._mock_faux_transaction_unconfirmed)
        self.register('transaction_send_result', self._mock_transaction_send_result)
        self.register('transaction_cancellation', self._mock_transaction_cancellation)
        
        # Balance and validation callbacks
        self.register('balance_updated', self._mock_balance_updated)
        self.register('txo_validation_complete', self._mock_txo_validation_complete)
        self.register('transaction_validation_complete', self._mock_transaction_validation_complete)
        
        # Network and communication callbacks
        self.register('connectivity_status', self._mock_connectivity_status)
        self.register('base_node_state', self._mock_base_node_state)
        self.register('contacts_liveness_data_updated', self._mock_contacts_liveness_data_updated)
        self.register('saf_messages_received', self._mock_saf_messages_received)
        
        # Scanning callbacks
        self.register('wallet_scanned_height', self._mock_wallet_scanned_height)
    
    def register(self, name: str, callback: Callable):
        """Register a callback function."""
        with self.lock:
            self.callbacks[name] = callback
            if name not in self.stats:
                self.stats[name] = CallbackStats(name=name)
    
    def get_callback(self, name: str) -> Optional[Callable]:
        """Get a registered callback by name."""
        return self.callbacks.get(name)
    
    def invoke(self, name: str, *args, **kwargs) -> Any:
        """Invoke a callback and track the invocation."""
        with self.lock:
            if name not in self.callbacks:
                raise ValueError(f"Callback '{name}' not registered")
            
            callback = self.callbacks[name]
            invocation = CallbackInvocation(
                callback_name=name,
                timestamp=time.time(),
                args=args,
                kwargs=kwargs
            )
            
            try:
                result = callback(*args, **kwargs)
                invocation.processed = True
                return result
            except Exception as e:
                invocation.error = str(e)
                raise
            finally:
                self.invocations.append(invocation)
                self._update_stats(name, invocation)
    
    def _update_stats(self, name: str, invocation: CallbackInvocation):
        """Update statistics for a callback."""
        stats = self.stats[name]
        stats.total_invocations += 1
        stats.invocations.append(invocation)
        
        # Calculate average interval
        if stats.last_invocation is not None:
            interval = invocation.timestamp - stats.last_invocation
            if stats.average_interval is None:
                stats.average_interval = interval
            else:
                stats.average_interval = (stats.average_interval + interval) / 2
        
        stats.last_invocation = invocation.timestamp
    
    def get_stats(self, name: str) -> Optional[CallbackStats]:
        """Get statistics for a specific callback."""
        return self.stats.get(name)
    
    def get_all_stats(self) -> Dict[str, CallbackStats]:
        """Get statistics for all callbacks."""
        with self.lock:
            return self.stats.copy()
    
    def clear_stats(self):
        """Clear all invocation statistics."""
        with self.lock:
            self.invocations.clear()
            for stats in self.stats.values():
                stats.total_invocations = 0
                stats.last_invocation = None
                stats.average_interval = None
                stats.invocations.clear()
    
    def get_invocation_history(self, name: Optional[str] = None) -> List[CallbackInvocation]:
        """Get invocation history for a specific callback or all callbacks."""
        with self.lock:
            if name is None:
                return self.invocations.copy()
            else:
                return [inv for inv in self.invocations if inv.callback_name == name]
    
    # Mock callback implementations
    
    def _mock_received_transaction(self, tx_data: Dict[str, Any]):
        """Mock implementation of received_transaction callback."""
        print(f"[MOCK] Transaction received: ID={tx_data.get('tx_id', 'unknown')}, "
              f"Amount={tx_data.get('amount', 0)} µT")
        
        # Validate expected fields
        expected_fields = ['tx_id', 'source_address', 'amount', 'fee', 'message', 'timestamp']
        for field in expected_fields:
            if field not in tx_data:
                print(f"WARNING: Missing field '{field}' in transaction data")
        
        return {'status': 'processed', 'tx_id': tx_data.get('tx_id')}
    
    def _mock_received_transaction_reply(self, tx_data: Dict[str, Any]):
        """Mock implementation of received_transaction_reply callback."""
        print(f"[MOCK] Transaction reply received: ID={tx_data.get('tx_id', 'unknown')}")
        return {'status': 'reply_processed'}
    
    def _mock_received_finalized_transaction(self, tx_data: Dict[str, Any]):
        """Mock implementation of received_finalized_transaction callback."""
        print(f"[MOCK] Finalized transaction received: ID={tx_data.get('tx_id', 'unknown')}")
        return {'status': 'finalized'}
    
    def _mock_transaction_broadcast(self, tx_data: Dict[str, Any]):
        """Mock implementation of transaction_broadcast callback."""
        print(f"[MOCK] Transaction broadcast: ID={tx_data.get('tx_id', 'unknown')}")
        return {'status': 'broadcast'}
    
    def _mock_transaction_mined(self, tx_data: Dict[str, Any]):
        """Mock implementation of transaction_mined callback."""
        print(f"[MOCK] Transaction mined: ID={tx_data.get('tx_id', 'unknown')}")
        return {'status': 'mined'}
    
    def _mock_transaction_mined_unconfirmed(self, tx_data: Dict[str, Any], confirmations: int):
        """Mock implementation of transaction_mined_unconfirmed callback."""
        print(f"[MOCK] Transaction mined (unconfirmed): ID={tx_data.get('tx_id', 'unknown')}, "
              f"Confirmations={confirmations}")
        return {'status': 'mined_unconfirmed', 'confirmations': confirmations}
    
    def _mock_faux_transaction_confirmed(self, tx_data: Dict[str, Any]):
        """Mock implementation of faux_transaction_confirmed callback."""
        print(f"[MOCK] Faux transaction confirmed: ID={tx_data.get('tx_id', 'unknown')}")
        return {'status': 'faux_confirmed'}
    
    def _mock_faux_transaction_unconfirmed(self, tx_data: Dict[str, Any], confirmations: int):
        """Mock implementation of faux_transaction_unconfirmed callback."""
        print(f"[MOCK] Faux transaction unconfirmed: ID={tx_data.get('tx_id', 'unknown')}, "
              f"Confirmations={confirmations}")
        return {'status': 'faux_unconfirmed', 'confirmations': confirmations}
    
    def _mock_transaction_send_result(self, tx_id: int, status_data: Dict[str, Any]):
        """Mock implementation of transaction_send_result callback."""
        status = status_data.get('status', 'unknown')
        print(f"[MOCK] Transaction send result: ID={tx_id}, Status={status}")
        
        if status == 'Failed':
            error_reason = status_data.get('error_reason', 'Unknown error')
            print(f"       Error reason: {error_reason}")
        
        return {'status': 'result_processed', 'tx_id': tx_id}
    
    def _mock_transaction_cancellation(self, tx_data: Dict[str, Any], reason: int):
        """Mock implementation of transaction_cancellation callback."""
        print(f"[MOCK] Transaction cancelled: ID={tx_data.get('tx_id', 'unknown')}, "
              f"Reason={reason}")
        return {'status': 'cancelled', 'reason': reason}
    
    def _mock_balance_updated(self, balance_data: Dict[str, Any]):
        """Mock implementation of balance_updated callback."""
        available = balance_data.get('available', 0)
        pending_in = balance_data.get('pending_incoming', 0)
        pending_out = balance_data.get('pending_outgoing', 0)
        
        print(f"[MOCK] Balance updated: Available={available} µT, "
              f"Pending In={pending_in} µT, Pending Out={pending_out} µT")
        
        # Convert to XTR for display
        available_xtr = available / 1_000_000
        pending_in_xtr = pending_in / 1_000_000
        pending_out_xtr = pending_out / 1_000_000
        
        print(f"       In XTR: Available={available_xtr:.6f}, "
              f"Pending In={pending_in_xtr:.6f}, Pending Out={pending_out_xtr:.6f}")
        
        return {'status': 'balance_processed', 'available_xtr': available_xtr}
    
    def _mock_txo_validation_complete(self, request_key: int, result: int):
        """Mock implementation of txo_validation_complete callback."""
        status = "success" if result == 0 else f"error_{result}"
        print(f"[MOCK] TXO validation complete: Request={request_key}, Result={status}")
        return {'status': 'validation_processed', 'result': result}
    
    def _mock_transaction_validation_complete(self, request_key: int, result: int):
        """Mock implementation of transaction_validation_complete callback."""
        status = "success" if result == 0 else f"error_{result}"
        print(f"[MOCK] Transaction validation complete: Request={request_key}, Result={status}")
        return {'status': 'validation_processed', 'result': result}
    
    def _mock_connectivity_status(self, status: int):
        """Mock implementation of connectivity_status callback."""
        status_str = "online" if status == 1 else "offline"
        print(f"[MOCK] Connectivity status changed: {status_str}")
        return {'status': 'connectivity_processed', 'online': status == 1}
    
    def _mock_base_node_state(self, state_data: Dict[str, Any]):
        """Mock implementation of base_node_state callback."""
        height = state_data.get('best_block_height', 0)
        synced = state_data.get('is_node_synced', False)
        latency = state_data.get('latency', 0)
        
        print(f"[MOCK] Base node state: Height={height}, Synced={synced}, Latency={latency}ms")
        return {'status': 'state_processed', 'height': height, 'synced': synced}
    
    def _mock_contacts_liveness_data_updated(self, contact_data: Dict[str, Any]):
        """Mock implementation of contacts_liveness_data_updated callback."""
        address = contact_data.get('address', 'unknown')
        online = contact_data.get('online_status', None)
        print(f"[MOCK] Contact liveness updated: {address}, Online={online}")
        return {'status': 'contact_processed', 'address': address}
    
    def _mock_saf_messages_received(self):
        """Mock implementation of saf_messages_received callback."""
        print("[MOCK] SAF messages received")
        return {'status': 'saf_processed'}
    
    def _mock_wallet_scanned_height(self, height: int):
        """Mock implementation of wallet_scanned_height callback."""
        print(f"[MOCK] Wallet scanned height: {height}")
        return {'status': 'scan_processed', 'height': height}


# Singleton instance for use across tests
mock_callback_registry = MockCallbackRegistry()


def get_mock_callback(name: str) -> Callable:
    """Get a mock callback function by name."""
    return mock_callback_registry.get_callback(name)


def invoke_mock_callback(name: str, *args, **kwargs) -> Any:
    """Invoke a mock callback and track the invocation."""
    return mock_callback_registry.invoke(name, *args, **kwargs)


def clear_mock_callback_stats():
    """Clear all mock callback statistics."""
    mock_callback_registry.clear_stats()


def get_mock_callback_stats(name: Optional[str] = None) -> Dict[str, CallbackStats]:
    """Get mock callback statistics."""
    if name:
        stats = mock_callback_registry.get_stats(name)
        return {name: stats} if stats else {}
    else:
        return mock_callback_registry.get_all_stats()


def generate_mock_transaction_data(tx_id: int = None) -> Dict[str, Any]:
    """Generate mock transaction data for testing."""
    if tx_id is None:
        tx_id = int(time.time() * 1000) % 1000000  # Use timestamp-based ID
    
    return {
        'tx_id': tx_id,
        'source_address': '🎯🔥🌟💫🎸🎭🎨🎪🎯🔥🌟💫🎸🎭🎨🎪🎯🔥🌟💫🎸🎭🎨🎪🎯🔥🌟💫',
        'amount': 1000000,  # 1 XTR
        'fee': 10000,       # 0.01 XTR
        'message': f'Test transaction {tx_id}',
        'timestamp': datetime.now().isoformat(),
        'cancelled': None,
    }


def generate_mock_balance_data(available: int = None) -> Dict[str, Any]:
    """Generate mock balance data for testing."""
    if available is None:
        available = 5000000  # 5 XTR default
    
    return {
        'available': available,
        'pending_incoming': available // 10,  # 10% of available
        'pending_outgoing': available // 20,   # 5% of available
        'time_locked': None,
    }


def generate_mock_base_node_state() -> Dict[str, Any]:
    """Generate mock base node state data for testing."""
    return {
        'node_id': b'mock_node_id_12345678901234567890123456789012',
        'best_block_height': 150000,
        'best_block_hash': b'mock_block_hash_1234567890123456789012345678901',
        'best_block_timestamp': int(time.time()),
        'pruning_horizon': 1000,
        'pruned_height': 149000,
        'is_node_synced': True,
        'updated_at': int(time.time() * 1000),
        'latency': 50,  # 50ms
    }


# Test utilities

def test_all_mock_callbacks():
    """Test that all mock callbacks can be invoked without errors."""
    print("Testing all mock callbacks...")
    
    # Test transaction callbacks
    tx_data = generate_mock_transaction_data()
    invoke_mock_callback('received_transaction', tx_data)
    invoke_mock_callback('received_transaction_reply', tx_data)
    invoke_mock_callback('received_finalized_transaction', tx_data)
    invoke_mock_callback('transaction_broadcast', tx_data)
    invoke_mock_callback('transaction_mined', tx_data)
    invoke_mock_callback('transaction_mined_unconfirmed', tx_data, 3)
    invoke_mock_callback('faux_transaction_confirmed', tx_data)
    invoke_mock_callback('faux_transaction_unconfirmed', tx_data, 1)
    invoke_mock_callback('transaction_send_result', tx_data['tx_id'], {'status': 'Sent'})
    invoke_mock_callback('transaction_cancellation', tx_data, 0)
    
    # Test balance callback
    balance_data = generate_mock_balance_data()
    invoke_mock_callback('balance_updated', balance_data)
    
    # Test validation callbacks
    invoke_mock_callback('txo_validation_complete', 12345, 0)
    invoke_mock_callback('transaction_validation_complete', 67890, 0)
    
    # Test network callbacks
    invoke_mock_callback('connectivity_status', 1)
    base_node_state = generate_mock_base_node_state()
    invoke_mock_callback('base_node_state', base_node_state)
    
    # Test communication callbacks
    contact_data = {'address': '🎯🔥🌟💫🎸🎭🎨🎪', 'online_status': True}
    invoke_mock_callback('contacts_liveness_data_updated', contact_data)
    invoke_mock_callback('saf_messages_received')
    
    # Test scanning callback
    invoke_mock_callback('wallet_scanned_height', 150000)
    
    # Print statistics
    stats = get_mock_callback_stats()
    print(f"\nCallback invocation summary:")
    total_invocations = sum(s.total_invocations for s in stats.values())
    print(f"Total callbacks invoked: {total_invocations}")
    
    for name, stat in stats.items():
        if stat.total_invocations > 0:
            print(f"  {name}: {stat.total_invocations} invocations")
    
    print("All mock callbacks tested successfully!")


if __name__ == "__main__":
    # Run test when executed directly
    test_all_mock_callbacks()
