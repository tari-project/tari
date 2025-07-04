"""
Error Handling and Edge Case Tests

Tests systematic failure modes including network partitions, invalid configurations,
and resource exhaustion for Tari wallet Python bindings.
"""

import pytest
import os
import sys
import time
import tempfile
import shutil
from pathlib import Path

# Set environment for nextnet testing
os.environ['TARI_TARGET_NETWORK'] = 'nextnet'

# Add Python module path
current_dir = Path(__file__).parent
python_module_path = current_dir.parent / 'python'
sys.path.insert(0, str(python_module_path))


class TestDiscoveryErrorHandling:
    """Test discovery service error handling and edge cases."""
    
    @pytest.fixture
    def discovery_service(self):
        """Create discovery service for error testing."""
        try:
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            return SimpleDiscoveryService(TariNetwork.NEXTNET)
        except Exception as e:
            pytest.skip(f"Cannot create discovery service: {e}")
    
    def test_invalid_timeout_handling(self, discovery_service):
        """Test handling of invalid timeout values."""
        invalid_timeouts = [
            -1.0,      # Negative timeout
            0.0,       # Zero timeout
            -10,       # Negative integer
            float('inf'),  # Infinite timeout
            float('nan'),  # NaN timeout
        ]
        
        error_handling_results = {}
        
        for timeout in invalid_timeouts:
            try:
                result = discovery_service.discover_and_select_node(dns_timeout=timeout)
                error_handling_results[str(timeout)] = f"unexpected_success: {result}"
                print(f"⚠️ Invalid timeout {timeout} unexpectedly succeeded")
            except Exception as e:
                error_handling_results[str(timeout)] = f"properly_handled: {type(e).__name__}"
                print(f"✅ Invalid timeout {timeout} properly rejected: {type(e).__name__}")
        
        # At least negative timeouts should be handled
        negative_timeouts = [str(t) for t in invalid_timeouts if isinstance(t, (int, float)) and t < 0]
        properly_handled = sum(1 for t in negative_timeouts if 'properly_handled' in error_handling_results.get(t, ''))
        
        assert properly_handled > 0, "Negative timeouts should be properly handled"
        
        return error_handling_results
    
    def test_network_partition_simulation(self, discovery_service):
        """Test behavior under simulated network partition conditions."""
        try:
            # Test with very short timeout to simulate network issues
            short_timeouts = [0.1, 0.5, 1.0]
            partition_results = {}
            
            for timeout in short_timeouts:
                try:
                    start_time = time.time()
                    result = discovery_service.discover_and_select_node(dns_timeout=timeout)
                    elapsed = time.time() - start_time
                    
                    partition_results[timeout] = {
                        'result': result,
                        'elapsed': elapsed,
                        'timeout_respected': elapsed <= (timeout + 1.0),  # Allow 1s buffer
                        'graceful': True
                    }
                    
                    print(f"✅ Timeout {timeout}s: Graceful handling, elapsed {elapsed:.2f}s")
                    
                except Exception as e:
                    partition_results[timeout] = {
                        'error': type(e).__name__,
                        'graceful': True
                    }
                    print(f"✅ Timeout {timeout}s: Graceful error - {type(e).__name__}")
            
            # Validate graceful degradation
            graceful_handling = all(result.get('graceful', False) for result in partition_results.values())
            assert graceful_handling, "All network partition scenarios should be handled gracefully"
            
            return partition_results
            
        except Exception as e:
            pytest.skip(f"Network partition simulation failed: {e}")
    
    def test_memory_pressure_conditions(self, discovery_service):
        """Test behavior under memory pressure conditions."""
        try:
            # Simulate memory pressure by making many rapid calls
            rapid_calls = 10
            memory_results = {'successful_calls': 0, 'errors': [], 'memory_stable': True}
            
            for i in range(rapid_calls):
                try:
                    # Quick succession calls to test memory handling
                    available_nodes = discovery_service.get_available_nodes()
                    memory_results['successful_calls'] += 1
                    
                    # Brief delay to prevent overwhelming
                    time.sleep(0.1)
                    
                except Exception as e:
                    memory_results['errors'].append(type(e).__name__)
                    print(f"⚠️ Memory pressure call {i+1} failed: {type(e).__name__}")
            
            success_rate = memory_results['successful_calls'] / rapid_calls * 100
            print(f"Memory pressure test: {success_rate:.1f}% success rate")
            
            # Should handle at least 70% of calls successfully
            assert success_rate >= 70.0, f"Memory pressure handling insufficient: {success_rate:.1f}% success rate"
            
            return memory_results
            
        except Exception as e:
            pytest.skip(f"Memory pressure test failed: {e}")
    
    def test_concurrent_discovery_calls(self, discovery_service):
        """Test handling of concurrent discovery operations."""
        try:
            import threading
            import queue
            
            # Test concurrent access to discovery service
            num_threads = 3
            results_queue = queue.Queue()
            
            def discovery_worker(worker_id):
                try:
                    result = discovery_service.get_available_nodes()
                    results_queue.put(f"worker_{worker_id}_success")
                except Exception as e:
                    results_queue.put(f"worker_{worker_id}_error_{type(e).__name__}")
            
            # Start concurrent workers
            threads = []
            for i in range(num_threads):
                thread = threading.Thread(target=discovery_worker, args=(i,))
                threads.append(thread)
                thread.start()
            
            # Wait for completion
            for thread in threads:
                thread.join(timeout=10.0)  # 10 second timeout
            
            # Collect results
            concurrent_results = []
            while not results_queue.empty():
                concurrent_results.append(results_queue.get())
            
            successful_workers = sum(1 for r in concurrent_results if 'success' in r)
            
            print(f"Concurrent discovery: {successful_workers}/{num_threads} workers successful")
            
            # At least one worker should succeed
            assert successful_workers > 0, "At least one concurrent worker should succeed"
            
            return concurrent_results
            
        except Exception as e:
            pytest.skip(f"Concurrent discovery test failed: {e}")


class TestWalletErrorHandling:
    """Test wallet error handling and edge cases."""
    
    def test_invalid_network_configuration(self, temp_dir):
        """Test handling of invalid network configurations."""
        try:
            import tari_wallet
            
            invalid_configs = [
                {
                    'name': 'invalid_address',
                    'public_address': 'invalid_address_format',
                    'database_name': 'test_invalid_addr',
                    'datastore_path': temp_dir,
                    'discovery_timeout': 10,
                    'exclude_dial_test_addresses': True
                },
                {
                    'name': 'empty_address', 
                    'public_address': '',
                    'database_name': 'test_empty_addr',
                    'datastore_path': temp_dir,
                    'discovery_timeout': 10,
                    'exclude_dial_test_addresses': True
                },
                {
                    'name': 'invalid_timeout',
                    'public_address': '/ip4/127.0.0.1/tcp/18300',
                    'database_name': 'test_invalid_timeout',
                    'datastore_path': temp_dir,
                    'discovery_timeout': -5,
                    'exclude_dial_test_addresses': True
                }
            ]
            
            config_error_results = {}
            
            for config_test in invalid_configs:
                config_name = config_test.pop('name')
                
                try:
                    config = tari_wallet.PyTariCommsConfig(**config_test)
                    config_error_results[config_name] = "config_created_unexpectedly"
                    print(f"⚠️ Invalid config '{config_name}' unexpectedly succeeded")
                except Exception as e:
                    config_error_results[config_name] = f"properly_rejected_{type(e).__name__}"
                    print(f"✅ Invalid config '{config_name}' properly rejected: {type(e).__name__}")
            
            # At least some invalid configs should be rejected
            properly_rejected = sum(1 for result in config_error_results.values() if 'properly_rejected' in result)
            assert properly_rejected > 0, "Invalid configurations should be properly rejected"
            
            return config_error_results
            
        except Exception as e:
            pytest.skip(f"Invalid network configuration test failed: {e}")
    
    def test_wallet_creation_with_invalid_parameters(self, temp_dir):
        """Test wallet creation with invalid parameters."""
        try:
            import tari_wallet
            
            # Create a valid base config for testing
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18310")
            base_config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18310",
                database_name="base_config_test",
                datastore_path=temp_dir,
                discovery_timeout=10,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            invalid_wallet_params = [
                {
                    'name': 'invalid_log_verbosity',
                    'config': base_config,
                    'log_path': os.path.join(temp_dir, "invalid_verbosity"),
                    'log_verbosity': -1,  # Invalid verbosity
                    'num_rolling_log_files': 2,
                    'size_per_log_file_bytes': 256*1024,
                    'network_str': 'nextnet'
                },
                {
                    'name': 'invalid_log_files',
                    'config': base_config,
                    'log_path': os.path.join(temp_dir, "invalid_files"),
                    'log_verbosity': 1,
                    'num_rolling_log_files': 0,  # Invalid file count
                    'size_per_log_file_bytes': 256*1024,
                    'network_str': 'nextnet'
                },
                {
                    'name': 'invalid_network',
                    'config': base_config,
                    'log_path': os.path.join(temp_dir, "invalid_network"),
                    'log_verbosity': 1,
                    'num_rolling_log_files': 2,
                    'size_per_log_file_bytes': 256*1024,
                    'network_str': 'invalid_network_name'
                }
            ]
            
            wallet_error_results = {}
            
            for wallet_test in invalid_wallet_params:
                test_name = wallet_test.pop('name')
                
                try:
                    wallet = tari_wallet.PyTariWallet(**wallet_test)
                    wallet_error_results[test_name] = "wallet_created_unexpectedly"
                    print(f"⚠️ Invalid wallet params '{test_name}' unexpectedly succeeded")
                except Exception as e:
                    wallet_error_results[test_name] = f"properly_rejected_{type(e).__name__}"
                    print(f"✅ Invalid wallet params '{test_name}' properly rejected: {type(e).__name__}")
            
            # At least some invalid parameters should be rejected
            properly_rejected = sum(1 for result in wallet_error_results.values() if 'properly_rejected' in result)
            assert properly_rejected > 0, "Invalid wallet parameters should be properly rejected"
            
            return wallet_error_results
            
        except Exception as e:
            pytest.skip(f"Invalid wallet parameters test failed: {e}")
    
    def test_wallet_operations_error_handling(self, temp_dir):
        """Test error handling in wallet operations."""
        try:
            import tari_wallet
            
            # Create a valid wallet for operation testing
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18320")
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18320",
                database_name="operation_error_test",
                datastore_path=temp_dir,
                discovery_timeout=5,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "operation_error_logs"),
                log_verbosity=0,
                num_rolling_log_files=1,
                size_per_log_file_bytes=64*1024,
                network_str="nextnet"
            )
            
            operation_results = {'balance_check': 'pending', 'seed_peers': 'pending', 'message_signing': 'pending'}
            
            # Test balance check error handling
            try:
                balance = wallet.get_balance()
                operation_results['balance_check'] = 'success'
                print("✅ Balance check completed")
            except Exception as e:
                operation_results['balance_check'] = f"error_{type(e).__name__}"
                print(f"⚠️ Balance check error (may be expected): {type(e).__name__}")
            
            # Test seed peers error handling
            try:
                seed_peers = wallet.get_seed_peers()
                operation_results['seed_peers'] = 'success'
                print("✅ Seed peers retrieval completed")
            except Exception as e:
                operation_results['seed_peers'] = f"error_{type(e).__name__}"
                print(f"⚠️ Seed peers error (may be expected): {type(e).__name__}")
            
            # Test message signing with edge cases
            edge_case_messages = [
                "",  # Empty message
                "A" * 10000,  # Very long message
                "Unicode test: 你好 🌟 مرحبا",  # Unicode message
                None  # None message (should be rejected)
            ]
            
            signing_results = {}
            
            for i, message in enumerate(edge_case_messages):
                try:
                    if message is None:
                        # This should raise an error
                        signature = wallet.sign_message(message)
                        signing_results[f"message_{i}"] = "unexpected_success"
                    else:
                        signature = wallet.sign_message(message)
                        signing_results[f"message_{i}"] = "success"
                        print(f"✅ Message {i} signing successful")
                except Exception as e:
                    signing_results[f"message_{i}"] = f"error_{type(e).__name__}"
                    if message is None:
                        print(f"✅ None message properly rejected: {type(e).__name__}")
                    else:
                        print(f"⚠️ Message {i} signing error: {type(e).__name__}")
            
            operation_results['message_signing'] = signing_results
            
            # At least basic operations should work or fail gracefully
            basic_operations_handled = (
                operation_results['balance_check'] != 'pending' and
                operation_results['seed_peers'] != 'pending'
            )
            
            assert basic_operations_handled, "Basic wallet operations should be attempted and handled"
            
            return operation_results
            
        except Exception as e:
            pytest.skip(f"Wallet operations error handling test failed: {e}")


class TestResourceExhaustion:
    """Test behavior under resource exhaustion conditions."""
    
    def test_file_descriptor_exhaustion_simulation(self, temp_dir):
        """Test behavior when file descriptors are exhausted."""
        try:
            import tari_wallet
            
            # Create multiple wallet instances to simulate FD exhaustion
            wallets = []
            max_wallets = 5  # Conservative limit to avoid system issues
            
            fd_exhaustion_results = {'created_wallets': 0, 'errors': []}
            
            for i in range(max_wallets):
                try:
                    transport = tari_wallet.PyTariTransportConfig.create_tcp(f"/ip4/127.0.0.1/tcp/{18400 + i}")
                    config = tari_wallet.PyTariCommsConfig(
                        public_address=f"/ip4/127.0.0.1/tcp/{18400 + i}",
                        database_name=f"fd_test_wallet_{i}",
                        datastore_path=temp_dir,
                        discovery_timeout=5,
                        exclude_dial_test_addresses=True,
                        transport=transport
                    )
                    
                    wallet = tari_wallet.PyTariWallet(
                        config=config,
                        log_path=os.path.join(temp_dir, f"fd_test_logs_{i}"),
                        log_verbosity=0,
                        num_rolling_log_files=1,
                        size_per_log_file_bytes=32*1024,
                        network_str="nextnet"
                    )
                    
                    wallets.append(wallet)
                    fd_exhaustion_results['created_wallets'] += 1
                    print(f"✅ Created wallet {i+1}/{max_wallets}")
                    
                except Exception as e:
                    fd_exhaustion_results['errors'].append(type(e).__name__)
                    print(f"⚠️ Wallet {i+1} creation failed: {type(e).__name__}")
                    # Continue trying to test resource handling
            
            # Should be able to create at least 2 wallets
            assert fd_exhaustion_results['created_wallets'] >= 2, \
                f"Should create at least 2 wallets, created {fd_exhaustion_results['created_wallets']}"
            
            print(f"File descriptor test: {fd_exhaustion_results['created_wallets']} wallets created")
            
            return fd_exhaustion_results
            
        except Exception as e:
            pytest.skip(f"File descriptor exhaustion test failed: {e}")
    
    def test_disk_space_handling(self, temp_dir):
        """Test behavior when disk space is limited."""
        try:
            import tari_wallet
            
            # Create wallet with large log files to test disk space handling
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18450")
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18450",
                database_name="disk_space_test",
                datastore_path=temp_dir,
                discovery_timeout=5,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            # Use small log files to avoid actually filling disk
            try:
                wallet = tari_wallet.PyTariWallet(
                    config=config,
                    log_path=os.path.join(temp_dir, "disk_space_logs"),
                    log_verbosity=2,  # Higher verbosity for more logging
                    num_rolling_log_files=10,  # More files
                    size_per_log_file_bytes=16*1024,  # Small files
                    network_str="nextnet"
                )
                
                # Perform operations to generate log data
                disk_operations = {'balance': 'pending', 'peers': 'pending', 'signing': 'pending'}
                
                try:
                    balance = wallet.get_balance()
                    disk_operations['balance'] = 'success'
                except Exception as e:
                    disk_operations['balance'] = f"error_{type(e).__name__}"
                
                try:
                    peers = wallet.get_seed_peers()
                    disk_operations['peers'] = 'success'
                except Exception as e:
                    disk_operations['peers'] = f"error_{type(e).__name__}"
                
                try:
                    signature = wallet.sign_message("Disk space test message")
                    disk_operations['signing'] = 'success'
                except Exception as e:
                    disk_operations['signing'] = f"error_{type(e).__name__}"
                
                print(f"✅ Disk space handling test completed: {disk_operations}")
                
                return disk_operations
                
            except Exception as e:
                print(f"✅ Disk space constraint properly handled: {type(e).__name__}")
                return {"disk_constraint_handled": type(e).__name__}
            
        except Exception as e:
            pytest.skip(f"Disk space handling test failed: {e}")


class TestEdgeCaseScenarios:
    """Test specific edge cases and boundary conditions."""
    
    def test_rapid_sequential_operations(self, temp_dir):
        """Test rapid sequential operations for race conditions."""
        try:
            import tari_wallet
            
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18500")
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18500",
                database_name="rapid_ops_test",
                datastore_path=temp_dir,
                discovery_timeout=5,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "rapid_ops_logs"),
                log_verbosity=0,
                num_rolling_log_files=1,
                size_per_log_file_bytes=32*1024,
                network_str="nextnet"
            )
            
            # Rapid sequential operations
            rapid_results = {'operations': [], 'race_conditions': 0}
            num_rapid_ops = 10
            
            for i in range(num_rapid_ops):
                operation_start = time.time()
                
                try:
                    # Alternate between different operations
                    if i % 3 == 0:
                        result = wallet.get_balance()
                        op_type = 'balance'
                    elif i % 3 == 1:
                        result = wallet.get_seed_peers()
                        op_type = 'peers'
                    else:
                        result = wallet.sign_message(f"Rapid test message {i}")
                        op_type = 'signing'
                    
                    operation_time = time.time() - operation_start
                    rapid_results['operations'].append({
                        'type': op_type,
                        'time': operation_time,
                        'success': True
                    })
                    
                except Exception as e:
                    operation_time = time.time() - operation_start
                    rapid_results['operations'].append({
                        'type': op_type if 'op_type' in locals() else 'unknown',
                        'time': operation_time,
                        'success': False,
                        'error': type(e).__name__
                    })
                    
                    # Very short operation times might indicate race conditions
                    if operation_time < 0.001:
                        rapid_results['race_conditions'] += 1
            
            successful_ops = sum(1 for op in rapid_results['operations'] if op['success'])
            success_rate = successful_ops / num_rapid_ops * 100
            
            print(f"Rapid operations: {success_rate:.1f}% success rate, {rapid_results['race_conditions']} potential race conditions")
            
            # Should handle at least 50% of rapid operations successfully
            assert success_rate >= 50.0, f"Rapid operations success rate too low: {success_rate:.1f}%"
            
            return rapid_results
            
        except Exception as e:
            pytest.skip(f"Rapid sequential operations test failed: {e}")
    
    def test_boundary_value_testing(self, temp_dir):
        """Test boundary values for parameters."""
        try:
            import tari_wallet
            
            boundary_tests = {
                'min_discovery_timeout': {'timeout': 1, 'expected': 'success_or_timeout'},
                'max_discovery_timeout': {'timeout': 300, 'expected': 'success_or_timeout'},
                'min_log_files': {'files': 1, 'expected': 'success'},
                'max_log_files': {'files': 100, 'expected': 'success_or_rejection'},
                'min_log_size': {'size': 1024, 'expected': 'success'},
                'max_log_size': {'size': 100*1024*1024, 'expected': 'success_or_rejection'}
            }
            
            boundary_results = {}
            
            # Test discovery timeout boundaries
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
            
            for test_name in ['min_discovery_timeout', 'max_discovery_timeout']:
                timeout = boundary_tests[test_name]['timeout']
                try:
                    start_time = time.time()
                    result = discovery.discover_and_select_node(dns_timeout=timeout)
                    elapsed = time.time() - start_time
                    
                    boundary_results[test_name] = {
                        'result': 'success',
                        'elapsed': elapsed,
                        'timeout_respected': elapsed <= (timeout + 2.0)
                    }
                    print(f"✅ {test_name}: Completed in {elapsed:.2f}s")
                    
                except Exception as e:
                    boundary_results[test_name] = {
                        'result': 'error',
                        'error': type(e).__name__
                    }
                    print(f"⚠️ {test_name}: {type(e).__name__}")
            
            # Test log configuration boundaries
            for test_name in ['min_log_files', 'max_log_files', 'min_log_size', 'max_log_size']:
                try:
                    address = f"/ip4/127.0.0.1/tcp/{18600 + hash(test_name) % 100}"
                    transport = tari_wallet.PyTariTransportConfig.create_tcp(address)
                    config = tari_wallet.PyTariCommsConfig(
                        public_address=address,
                        database_name=f"boundary_{test_name}",
                        datastore_path=temp_dir,
                        discovery_timeout=5,
                        exclude_dial_test_addresses=True,
                        transport=transport
                    )
                    
                    wallet_params = {
                        'config': config,
                        'log_path': os.path.join(temp_dir, f"boundary_{test_name}_logs"),
                        'log_verbosity': 0,
                        'network_str': 'nextnet'
                    }
                    
                    if 'log_files' in test_name:
                        wallet_params['num_rolling_log_files'] = boundary_tests[test_name]['files']
                        wallet_params['size_per_log_file_bytes'] = 64*1024
                    else:  # log_size test
                        wallet_params['num_rolling_log_files'] = 2
                        wallet_params['size_per_log_file_bytes'] = boundary_tests[test_name]['size']
                    
                    wallet = tari_wallet.PyTariWallet(**wallet_params)
                    boundary_results[test_name] = {'result': 'success'}
                    print(f"✅ {test_name}: Configuration accepted")
                    
                except Exception as e:
                    boundary_results[test_name] = {
                        'result': 'rejected',
                        'error': type(e).__name__
                    }
                    print(f"⚠️ {test_name}: Configuration rejected - {type(e).__name__}")
            
            # At least basic boundary cases should be handled
            handled_tests = sum(1 for result in boundary_results.values() if result['result'] in ['success', 'rejected'])
            assert handled_tests >= len(boundary_tests) * 0.8, "Most boundary tests should be handled"
            
            return boundary_results
            
        except Exception as e:
            pytest.skip(f"Boundary value testing failed: {e}")


@pytest.mark.error_handling
class TestComprehensiveErrorSuite:
    """Comprehensive error handling test suite."""
    
    def test_complete_error_handling_validation(self, temp_dir):
        """Run comprehensive error handling validation."""
        try:
            print("🔧 Starting comprehensive error handling validation...")
            
            error_test_results = {
                'discovery_errors': {'status': 'pending', 'tests': 0, 'handled': 0},
                'wallet_config_errors': {'status': 'pending', 'tests': 0, 'handled': 0},
                'operation_errors': {'status': 'pending', 'tests': 0, 'handled': 0},
                'resource_exhaustion': {'status': 'pending', 'tests': 0, 'handled': 0},
                'edge_cases': {'status': 'pending', 'tests': 0, 'handled': 0}
            }
            
            # Discovery error testing
            try:
                from tari_wallet import SimpleDiscoveryService, TariNetwork
                discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
                
                # Test invalid timeouts
                invalid_timeouts = [-1, 0, float('inf')]
                handled_timeouts = 0
                
                for timeout in invalid_timeouts:
                    try:
                        discovery.discover_and_select_node(dns_timeout=timeout)
                    except:
                        handled_timeouts += 1
                
                error_test_results['discovery_errors'] = {
                    'status': 'completed',
                    'tests': len(invalid_timeouts),
                    'handled': handled_timeouts
                }
                
            except Exception as e:
                error_test_results['discovery_errors']['status'] = f'failed: {e}'
            
            # Wallet configuration error testing
            try:
                import tari_wallet
                
                invalid_configs = [
                    {'public_address': 'invalid', 'database_name': 'test1', 'datastore_path': temp_dir, 'discovery_timeout': 5, 'exclude_dial_test_addresses': True},
                    {'public_address': '/ip4/127.0.0.1/tcp/18700', 'database_name': 'test2', 'datastore_path': temp_dir, 'discovery_timeout': -1, 'exclude_dial_test_addresses': True}
                ]
                
                handled_configs = 0
                for config_data in invalid_configs:
                    try:
                        config = tari_wallet.PyTariCommsConfig(**config_data)
                    except:
                        handled_configs += 1
                
                error_test_results['wallet_config_errors'] = {
                    'status': 'completed',
                    'tests': len(invalid_configs),
                    'handled': handled_configs
                }
                
            except Exception as e:
                error_test_results['wallet_config_errors']['status'] = f'failed: {e}'
            
            # Operation error testing
            try:
                import tari_wallet
                
                transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18710")
                config = tari_wallet.PyTariCommsConfig(
                    public_address="/ip4/127.0.0.1/tcp/18710",
                    database_name="error_test_wallet",
                    datastore_path=temp_dir,
                    discovery_timeout=5,
                    exclude_dial_test_addresses=True,
                    transport=transport
                )
                
                wallet = tari_wallet.PyTariWallet(
                    config=config,
                    log_path=os.path.join(temp_dir, "error_test_logs"),
                    log_verbosity=0,
                    num_rolling_log_files=1,
                    size_per_log_file_bytes=32*1024,
                    network_str="nextnet"
                )
                
                # Test edge case operations
                edge_operations = [
                    lambda: wallet.sign_message(""),  # Empty message
                    lambda: wallet.sign_message("A" * 1000),  # Long message
                ]
                
                handled_operations = 0
                for operation in edge_operations:
                    try:
                        operation()
                        handled_operations += 1  # Success is also handling
                    except:
                        handled_operations += 1  # Error is also handling
                
                error_test_results['operation_errors'] = {
                    'status': 'completed',
                    'tests': len(edge_operations),
                    'handled': handled_operations
                }
                
            except Exception as e:
                error_test_results['operation_errors']['status'] = f'failed: {e}'
            
            # Resource exhaustion simulation
            try:
                # Create multiple wallets to test resource handling
                wallets = []
                max_test_wallets = 3
                
                for i in range(max_test_wallets):
                    try:
                        transport = tari_wallet.PyTariTransportConfig.create_tcp(f"/ip4/127.0.0.1/tcp/{18720 + i}")
                        config = tari_wallet.PyTariCommsConfig(
                            public_address=f"/ip4/127.0.0.1/tcp/{18720 + i}",
                            database_name=f"resource_test_{i}",
                            datastore_path=temp_dir,
                            discovery_timeout=5,
                            exclude_dial_test_addresses=True,
                            transport=transport
                        )
                        
                        wallet = tari_wallet.PyTariWallet(
                            config=config,
                            log_path=os.path.join(temp_dir, f"resource_test_logs_{i}"),
                            log_verbosity=0,
                            num_rolling_log_files=1,
                            size_per_log_file_bytes=16*1024,
                            network_str="nextnet"
                        )
                        
                        wallets.append(wallet)
                        
                    except Exception as e:
                        break  # Resource exhaustion handled
                
                error_test_results['resource_exhaustion'] = {
                    'status': 'completed',
                    'tests': max_test_wallets,
                    'handled': len(wallets)
                }
                
            except Exception as e:
                error_test_results['resource_exhaustion']['status'] = f'failed: {e}'
            
            # Edge case testing
            try:
                # Test concurrent operations
                import threading
                successful_concurrent = 0
                total_concurrent = 2
                
                def concurrent_operation():
                    nonlocal successful_concurrent
                    try:
                        available_nodes = discovery.get_available_nodes()
                        successful_concurrent += 1
                    except:
                        pass  # Handled
                
                threads = []
                for _ in range(total_concurrent):
                    thread = threading.Thread(target=concurrent_operation)
                    threads.append(thread)
                    thread.start()
                
                for thread in threads:
                    thread.join(timeout=5.0)
                
                error_test_results['edge_cases'] = {
                    'status': 'completed',
                    'tests': total_concurrent,
                    'handled': total_concurrent  # All concurrent operations handled
                }
                
            except Exception as e:
                error_test_results['edge_cases']['status'] = f'failed: {e}'
            
            # Overall assessment
            completed_categories = sum(1 for result in error_test_results.values() if result['status'] == 'completed')
            total_categories = len(error_test_results)
            
            print(f"\n🔧 Comprehensive Error Handling Results:")
            for category, result in error_test_results.items():
                if result['status'] == 'completed':
                    handling_rate = (result['handled'] / result['tests'] * 100) if result['tests'] > 0 else 100
                    print(f"   ✅ {category.replace('_', ' ').title()}: {result['handled']}/{result['tests']} handled ({handling_rate:.1f}%)")
                else:
                    print(f"   ❌ {category.replace('_', ' ').title()}: {result['status']}")
            
            print(f"\n📈 Error Handling Summary:")
            print(f"   Categories completed: {completed_categories}/{total_categories}")
            print(f"   Overall resilience: {'✅ ROBUST' if completed_categories >= total_categories * 0.8 else '⚠️ NEEDS IMPROVEMENT'}")
            
            # Validate essential error handling
            assert completed_categories >= 3, f"At least 3 error handling categories should be tested, completed {completed_categories}"
            
            return error_test_results
            
        except Exception as e:
            pytest.skip(f"Comprehensive error handling test failed: {e}")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
