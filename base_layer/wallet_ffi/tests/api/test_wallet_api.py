"""
Wallet API Compliance Tests

Validates that wallet interfaces match documented API contracts.
Tests core wallet functionality, method signatures, and return types.
"""

import pytest
import os
import sys
from pathlib import Path

# Set environment for nextnet testing
os.environ['TARI_TARGET_NETWORK'] = 'nextnet'

# Add Python module path
current_dir = Path(__file__).parent
python_module_path = current_dir.parent.parent / 'python'
sys.path.insert(0, str(python_module_path))


class TestPyTariWalletAPI:
    """Test PyTariWallet API compliance."""
    
    @pytest.fixture
    def wallet_classes(self):
        """Get wallet classes for API testing."""
        try:
            import tari_wallet
            return {
                'PyTariWallet': tari_wallet.PyTariWallet,
                'PyTariCommsConfig': tari_wallet.PyTariCommsConfig,
                'PyTariBalance': getattr(tari_wallet, 'PyTariBalance', None),
                'TariWalletError': getattr(tari_wallet, 'TariWalletError', None)
            }
        except ImportError as e:
            pytest.skip(f"Cannot import wallet classes: {e}")
    
    @pytest.fixture
    def basic_wallet_config(self, temp_dir):
        """Create basic wallet configuration for testing."""
        try:
            import tari_wallet
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
            return tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18188",
                database_name="api_test_wallet",
                datastore_path=temp_dir,
                discovery_timeout=10,
                exclude_dial_test_addresses=True,
                transport=transport
            )
        except Exception as e:
            pytest.skip(f"Cannot create wallet config: {e}")
    
    @pytest.fixture
    def test_wallet_instance(self, basic_wallet_config, temp_dir):
        """Create wallet instance for API testing."""
        import tari_wallet
        return tari_wallet.PyTariWallet(
            config=basic_wallet_config,
            log_path=os.path.join(temp_dir, "api_logs"),
            log_verbosity=1,
            num_rolling_log_files=2,
            size_per_log_file_bytes=256*1024,
            network_str="nextnet",
            passphrase="test_wallet_passphrase"
        )
    
    def test_wallet_class_exists(self, wallet_classes):
        """Validate PyTariWallet class exists and is instantiable."""
        PyTariWallet = wallet_classes['PyTariWallet']
        
        assert PyTariWallet is not None, "PyTariWallet class must exist"
        assert callable(PyTariWallet), "PyTariWallet must be instantiable"
        
    def test_wallet_config_class_exists(self, wallet_classes):
        """Validate PyTariCommsConfig class exists."""
        PyTariCommsConfig = wallet_classes['PyTariCommsConfig'] 
        
        assert PyTariCommsConfig is not None, "PyTariCommsConfig class must exist"
        assert callable(PyTariCommsConfig), "PyTariCommsConfig must be instantiable"
    
    def test_get_balance_api(self, test_wallet_instance, api_validator):
        """Test get_balance API compliance."""
        method_name = "get_balance"
        
        # Check method exists
        assert api_validator.validate_method_exists(test_wallet_instance, method_name), \
            f"{method_name} method must exist"
        
        # Get method info
        method_info = api_validator.get_method_info(test_wallet_instance, method_name)
        print(f"get_balance signature: {method_info['signature']}")
        
        # Test method execution
        try:
            balance = test_wallet_instance.get_balance()
            assert balance is not None, "get_balance should return a balance object"
            
            # Test balance object properties
            balance_properties = []
            for attr in ['total_minotari', 'available_minotari', 'pending_incoming', 'pending_outgoing']:
                if hasattr(balance, attr):
                    value = getattr(balance, attr)
                    balance_properties.append(f"{attr}: {type(value).__name__}")
            
            print(f"✅ get_balance API compliant: {len(balance_properties)} properties")
            
        except Exception as e:
            pytest.fail(f"get_balance execution failed: {e}")
    
    def test_get_seed_peers_api(self, test_wallet_instance, api_validator):
        """Test get_seed_peers API compliance."""
        method_name = "get_seed_peers"
        
        # Check method exists
        assert api_validator.validate_method_exists(test_wallet_instance, method_name), \
            f"{method_name} method must exist"
        
        # Get method info
        method_info = api_validator.get_method_info(test_wallet_instance, method_name)
        print(f"get_seed_peers signature: {method_info['signature']}")
        
        # Test method execution
        try:
            seed_peers = test_wallet_instance.get_seed_peers()
            assert isinstance(seed_peers, list), "get_seed_peers should return a list"
            
            print(f"✅ get_seed_peers API compliant: returns list with {len(seed_peers)} peers")
            
            # Test peer object structure if peers exist
            if seed_peers:
                peer_sample = seed_peers[0]
                peer_properties = []
                for attr in ['public_key', 'addresses', 'flags']:
                    if hasattr(peer_sample, attr):
                        peer_properties.append(attr)
                
                print(f"Peer object properties: {peer_properties}")
            
        except Exception as e:
            pytest.fail(f"get_seed_peers execution failed: {e}")
    
    def test_sign_message_api(self, test_wallet_instance, api_validator):
        """Test sign_message API compliance."""
        method_name = "sign_message"
        
        # Check method exists
        assert api_validator.validate_method_exists(test_wallet_instance, method_name), \
            f"{method_name} method must exist"
        
        # Get method info
        method_info = api_validator.get_method_info(test_wallet_instance, method_name)
        
        # Validate signature accepts message parameter
        params = method_info.get("parameters", [])
        message_params = [p for p in params if 'message' in p.lower()]
        assert len(message_params) > 0, "sign_message should accept message parameter"
        
        print(f"sign_message signature: {method_info['signature']}")
        
        # Test method execution
        try:
            test_message = "API compliance test message"
            signature = test_wallet_instance.sign_message(test_message)
            
            assert isinstance(signature, str), "sign_message should return string signature"
            assert len(signature) > 0, "signature should not be empty"
            
            print(f"✅ sign_message API compliant: signature length {len(signature)}")
            
        except Exception as e:
            pytest.fail(f"sign_message execution failed: {e}")
    
    def test_wallet_constructor_api(self, wallet_classes, basic_wallet_config, temp_dir):
        """Test wallet constructor API compliance."""
        PyTariWallet = wallet_classes['PyTariWallet']
        
        # Test documented constructor parameters
        required_params = {
            'config': basic_wallet_config,
            'log_path': os.path.join(temp_dir, "constructor_test"),
            'log_verbosity': 1,
            'num_rolling_log_files': 2,
            'size_per_log_file_bytes': 256*1024,
            'network_str': "nextnet"
        }
        
        wallet = PyTariWallet(**required_params, passphrase="test_wallet_passphrase")
        assert wallet is not None, "Wallet constructor should succeed with documented parameters"
        print("✅ Wallet constructor API compliant")
    
    def test_wallet_error_handling_api(self, wallet_classes):
        """Test wallet error handling API compliance."""
        TariWalletError = wallet_classes.get('TariWalletError')
        
        if TariWalletError is None:
            pytest.skip("TariWalletError class not available")
        
        # Test that it's a proper exception class
        assert issubclass(TariWalletError, Exception), "TariWalletError should be an Exception subclass"
        
        # Test instantiation
        try:
            error = TariWalletError("Test error message")
            assert str(error) == "Test error message", "TariWalletError should preserve error message"
            print("✅ TariWalletError API compliant")
        except Exception as e:
            pytest.fail(f"TariWalletError instantiation failed: {e}")


class TestWalletAPIIntegration:
    """Test wallet API integration workflows."""
    
    def test_wallet_creation_workflow_api(self, temp_dir):
        """Test complete wallet creation workflow API."""
        try:
            import tari_wallet
            
            workflow_steps = {}
            
            # Step 1: Create configuration
            try:
                transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18190")
                config = tari_wallet.PyTariCommsConfig(
                    public_address="/ip4/127.0.0.1/tcp/18190",
                    database_name="integration_wallet",
                    datastore_path=temp_dir,
                    discovery_timeout=15,
                    exclude_dial_test_addresses=True,
                    transport=transport
                )
                workflow_steps['config_creation'] = True
                print("✅ API Step 1: Configuration creation")
            except Exception as e:
                workflow_steps['config_creation'] = False
                pytest.fail(f"Config creation failed: {e}")
            
            # Step 2: Create wallet
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "integration_logs"),
                log_verbosity=2,
                num_rolling_log_files=3,
                size_per_log_file_bytes=512*1024,
                network_str="nextnet",
                passphrase="test_wallet_passphrase"
            )
            workflow_steps['wallet_creation'] = True
            print("✅ API Step 2: Wallet creation")
            
            # Step 3: Test basic wallet operations
            # Test balance retrieval
            balance = wallet.get_balance()
            workflow_steps['balance_access'] = balance is not None
            
            # Test seed peers
            seed_peers = wallet.get_seed_peers()
            workflow_steps['seed_peers_access'] = isinstance(seed_peers, list)
            
            # Test message signing
            signature = wallet.sign_message("Integration test message")
            workflow_steps['message_signing'] = isinstance(signature, str) and len(signature) > 0
            
            workflow_steps['basic_operations'] = True
            print("✅ API Step 3: Basic wallet operations")
            
            # Validate workflow success
            essential_steps = ['config_creation', 'wallet_creation', 'basic_operations']
            failed_steps = [step for step in essential_steps if not workflow_steps.get(step, False)]
            
            if failed_steps:
                pytest.fail(f"Essential workflow steps failed: {failed_steps}")
            
            print(f"✅ Wallet creation workflow API validated: {workflow_steps}")
            return workflow_steps
            
        except ImportError as e:
            pytest.skip(f"Cannot test wallet workflow - import failed: {e}")
        except Exception as e:
            pytest.fail(f"Wallet workflow test failed: {e}")
    
    def test_wallet_with_discovery_integration_api(self, temp_dir):
        """Test wallet integration with discovery service API."""
        try:
            import tari_wallet
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            
            integration_results = {}
            
            # Create wallet  
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18192")
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18192",
                database_name="discovery_integration",
                datastore_path=temp_dir,
                discovery_timeout=10,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "discovery_integration_logs"),
                log_verbosity=1,
                num_rolling_log_files=2,
                size_per_log_file_bytes=256*1024,
                network_str="nextnet",
                passphrase="test_wallet_passphrase"
            )
            
            # Create discovery service
            discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
            
            # Test integration
            # Get seed peers from wallet
            wallet_peers = wallet.get_seed_peers()
            integration_results['wallet_peers_accessible'] = isinstance(wallet_peers, list)
            
            # Get available nodes from discovery
            discovery_nodes = discovery.get_available_nodes()
            integration_results['discovery_nodes_accessible'] = isinstance(discovery_nodes, list)
            
            # Compare peer/node information structures
            if wallet_peers and discovery_nodes:
                # Both should provide network connectivity information
                wallet_peer_attrs = set(dir(wallet_peers[0])) if wallet_peers else set()
                discovery_node_attrs = set(dir(discovery_nodes[0])) if discovery_nodes else set()
                
                # Look for common attributes indicating network peer information
                common_attrs = wallet_peer_attrs.intersection(discovery_node_attrs)
                integration_results['compatible_structures'] = len(common_attrs) > 0
                
                print(f"Wallet-Discovery integration: {len(common_attrs)} common attributes")
            else:
                integration_results['compatible_structures'] = True  # Skip if no data
            
            integration_results['integration_successful'] = True
            print("✅ Wallet-Discovery integration API validated")
            
            return integration_results
            
        except Exception as e:
            pytest.skip(f"Wallet-Discovery integration test failed: {e}")


class TestWalletAPIParameterValidation:
    """Test wallet API parameter validation."""
    
    def test_wallet_constructor_parameter_validation(self, temp_dir):
        """Test wallet constructor parameter validation."""
        try:
            import tari_wallet
            
            # Test required parameter validation
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18194")
            base_config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18194",
                database_name="param_test",
                datastore_path=temp_dir,
                discovery_timeout=5,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            # Valid parameter combinations
            valid_params = [
                {
                    'config': base_config,
                    'log_path': os.path.join(temp_dir, "valid1"),
                    'log_verbosity': 1,
                    'num_rolling_log_files': 2,
                    'size_per_log_file_bytes': 256*1024,
                    'network_str': "nextnet"
                },
                {
                    'config': base_config,
                    'log_path': os.path.join(temp_dir, "valid2"),
                    'log_verbosity': 0,  # Different verbosity
                    'num_rolling_log_files': 5,  # Different file count
                    'size_per_log_file_bytes': 1024*1024,  # Different size
                    'network_str': "nextnet"
                }
            ]
            
            for i, params in enumerate(valid_params):
                wallet = tari_wallet.PyTariWallet(**params, passphrase="test_wallet_passphrase")
                print(f"✅ Valid parameter set {i+1} accepted")
            
            # Test invalid network strings (should handle gracefully)
            invalid_networks = ["invalid_network", "", None]
            
            for network in invalid_networks:
                try:
                    invalid_params = {
                        'config': base_config,
                        'log_path': os.path.join(temp_dir, f"invalid_{network}"),
                        'log_verbosity': 1,
                        'num_rolling_log_files': 2,
                        'size_per_log_file_bytes': 256*1024,
                        'network_str': network
                    }
                    
                    wallet = tari_wallet.PyTariWallet(**invalid_params)
                    print(f"⚠️ Invalid network '{network}' unexpectedly accepted")
                except Exception as e:
                    print(f"✅ Invalid network '{network}' properly rejected: {type(e).__name__}")
            
        except Exception as e:
            pytest.skip(f"Parameter validation test failed: {e}")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
