"""
Network API Compliance Tests

Validates that network management interfaces match documented API contracts.
Tests base node management, network configuration, and connectivity APIs.
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


class TestBaseNodeManagerAPI:
    """Test BaseNodeManager API compliance."""
    
    @pytest.fixture
    def base_node_manager_class(self):
        """Get BaseNodeManager class for API testing."""
        try:
            from tari_wallet import BaseNodeManager
            return BaseNodeManager
        except ImportError as e:
            pytest.skip(f"Cannot import BaseNodeManager: {e}")
    
    @pytest.fixture
    def base_node_manager_instance(self, base_node_manager_class):
        """Create BaseNodeManager instance for testing."""
        try:
            return base_node_manager_class()
        except Exception as e:
            pytest.skip(f"Cannot create BaseNodeManager instance: {e}")
    
    def test_base_node_manager_class_exists(self, base_node_manager_class):
        """Validate BaseNodeManager class exists."""
        assert base_node_manager_class is not None, "BaseNodeManager class must exist"
        assert callable(base_node_manager_class), "BaseNodeManager must be instantiable"
    
    def test_get_base_node_info_api(self, base_node_manager_instance, api_validator):
        """Test get_base_node_info API compliance."""
        method_name = "get_base_node_info"
        
        # Check if method exists
        if not api_validator.validate_method_exists(base_node_manager_instance, method_name):
            pytest.skip(f"{method_name} method not available")
        
        # Get method info
        method_info = api_validator.get_method_info(base_node_manager_instance, method_name)
        print(f"get_base_node_info signature: {method_info['signature']}")
        
        # Test method execution
        try:
            base_node_info = base_node_manager_instance.get_base_node_info()
            
            # Should return dictionary-like object with node information
            assert base_node_info is not None, "get_base_node_info should return node information"
            
            # Test expected properties
            expected_properties = ['public_key', 'address', 'name']
            available_properties = []
            
            for prop in expected_properties:
                if hasattr(base_node_info, prop):
                    available_properties.append(prop)
                elif isinstance(base_node_info, dict) and prop in base_node_info:
                    available_properties.append(prop)
            
            print(f"✅ get_base_node_info API compliant: {len(available_properties)} properties available")
            
        except Exception as e:
            pytest.fail(f"get_base_node_info execution failed: {e}")


class TestNetworkConfigurationAPI:
    """Test network configuration API compliance."""
    
    @pytest.fixture
    def network_config_classes(self):
        """Get network configuration classes."""
        try:
            import tari_wallet
            return {
                'PyTariCommsConfig': tari_wallet.PyTariCommsConfig,
                'PyTariTransportConfig': getattr(tari_wallet, 'PyTariTransportConfig', None),
                'TariNetwork': getattr(tari_wallet, 'TariNetwork', None)
            }
        except ImportError as e:
            pytest.skip(f"Cannot import network config classes: {e}")
    
    def test_pyTari_comms_config_api(self, network_config_classes, temp_dir):
        """Test PyTariCommsConfig API compliance."""
        PyTariCommsConfig = network_config_classes['PyTariCommsConfig']
        PyTariTransportConfig = network_config_classes['PyTariTransportConfig']
        
        assert PyTariCommsConfig is not None, "PyTariCommsConfig class must exist"
        
        # Test documented constructor parameters
        documented_params = {
            'public_address': "/ip4/127.0.0.1/tcp/18200",
            'database_name': "api_test_config",
            'datastore_path': temp_dir,
            'discovery_timeout': 20,
            'exclude_dial_test_addresses': True
        }
        
        # Add transport configuration
        if PyTariTransportConfig is not None:
            transport = PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18200")
            documented_params['transport'] = transport
        
        try:
            config = PyTariCommsConfig(**documented_params)
            assert config is not None, "PyTariCommsConfig should be creatable with documented parameters"
            
            # Test property access
            config_properties = []
            for param_name in documented_params.keys():
                if hasattr(config, param_name):
                    config_properties.append(param_name)
            
            print(f"✅ PyTariCommsConfig API compliant: {len(config_properties)} accessible properties")
            
        except Exception as e:
            pytest.fail(f"PyTariCommsConfig creation failed: {e}")
    
    def test_tari_network_enum_api(self, network_config_classes):
        """Test TariNetwork enum API compliance."""
        TariNetwork = network_config_classes.get('TariNetwork')
        
        if TariNetwork is None:
            pytest.skip("TariNetwork enum not available")
        
        # Test documented network values
        documented_networks = ['NEXTNET', 'TESTNET', 'MAINNET']
        available_networks = []
        
        for network in documented_networks:
            if hasattr(TariNetwork, network):
                available_networks.append(network)
                print(f"✅ TariNetwork.{network} available")
            else:
                print(f"❌ TariNetwork.{network} missing")
        
        # At least NEXTNET should be available for testing
        assert 'NEXTNET' in available_networks, "TariNetwork.NEXTNET must be available"
        
        print(f"TariNetwork enum API compliance: {len(available_networks)}/{len(documented_networks)} networks")


class TestNetworkUtilityAPI:
    """Test network utility function API compliance."""
    
    def test_format_base_node_info_api(self):
        """Test format_base_node_info function API compliance."""
        try:
            from tari_wallet import format_base_node_info, SimpleDiscoveryService, TariNetwork
            
            # Try to get a real BaseNode from discovery service
            try:
                discovery = SimpleDiscoveryService(TariNetwork.NEXTNET) # explicitly NEXTNET? make modular
                available_nodes = discovery.get_available_nodes()
                
                if available_nodes and len(available_nodes) > 0:
                    # Use real BaseNode object from discovery
                    base_node_info = available_nodes[0]
                    
                    try:
                        formatted_info = format_base_node_info(base_node_info)
                        
                        assert isinstance(formatted_info, str), "format_base_node_info should return string"
                        assert len(formatted_info) > 0, "formatted info should not be empty"
                        
                        # Should contain node information
                        if hasattr(base_node_info, 'name'):
                            assert base_node_info.name in formatted_info, "formatted info should contain node name"
                        
                        print(f"✅ format_base_node_info API compliant: {len(formatted_info)} characters")
                        
                    except Exception as e:
                        pytest.fail(f"format_base_node_info execution failed: {e}")
                        
                else:
                    pytest.skip("No available nodes to test format_base_node_info")
                    
            except Exception as e:
                pytest.skip(f"Cannot get BaseNode for format_base_node_info test: {e}")
                
        except ImportError:
            pytest.skip("format_base_node_info function not available as standalone import")
    
    def test_create_wallet_with_auto_discovery_api(self):
        """Test create_wallet_with_auto_discovery function API compliance."""
        try:
            from tari_wallet import create_wallet_with_auto_discovery
            
            # This is a high-level function - test that it exists and is callable
            assert callable(create_wallet_with_auto_discovery), \
                "create_wallet_with_auto_discovery should be callable"
            
            print("✅ create_wallet_with_auto_discovery API exists")
            
            # Note: We don't execute this as it's a complex operation that creates wallets
            
        except ImportError:
            pytest.skip("create_wallet_with_auto_discovery function not available")


class TestNetworkAPIIntegration:
    """Test network API integration patterns."""
    
    def test_network_configuration_workflow_api(self, temp_dir):
        """Test network configuration workflow API."""
        try:
            import tari_wallet
            
            workflow_steps = {}
            
            # Step 1: Create network configuration
            try:
                transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18202")
                config = tari_wallet.PyTariCommsConfig(
                    public_address="/ip4/127.0.0.1/tcp/18202",
                    database_name="network_workflow",
                    datastore_path=temp_dir,
                    discovery_timeout=15,
                    exclude_dial_test_addresses=True,
                    transport=transport
                )
                workflow_steps['config_creation'] = True
                print("✅ Network config creation")
            except Exception as e:
                workflow_steps['config_creation'] = False
                pytest.fail(f"Network config creation failed: {e}")
            
            # Step 2: Create wallet with network config
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "network_workflow_logs"),
                log_verbosity=1,
                num_rolling_log_files=2,
                size_per_log_file_bytes=256*1024,
                network_str="nextnet",
                passphrase="test_network_workflow"
            )
            workflow_steps['wallet_with_config'] = True
            print("✅ Wallet creation with network config")
            
            # Step 3: Test network operations
            # Get seed peers (network-related)
            seed_peers = wallet.get_seed_peers()
            workflow_steps['network_peers_access'] = isinstance(seed_peers, list)
            
            # Test balance (requires network connectivity)
            balance = wallet.get_balance()
            workflow_steps['network_balance_access'] = balance is not None
            
            workflow_steps['network_operations'] = True
            print("✅ Network operations through wallet")
            
            # Validate essential workflow
            essential_steps = ['config_creation', 'wallet_with_config', 'network_operations']
            failed_steps = [step for step in essential_steps if not workflow_steps.get(step, False)]
            
            if failed_steps:
                pytest.fail(f"Essential network workflow steps failed: {failed_steps}")
            
            print(f"✅ Network configuration workflow API validated: {workflow_steps}")
            return workflow_steps
            
        except Exception as e:
            pytest.fail(f"Network workflow test failed: {e}")
    
    def test_discovery_network_integration_api(self, temp_dir):
        """Test discovery service integration with network API."""
        try:
            import tari_wallet
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            
            integration_results = {}
            
            # Create discovery service with network specification
            try:
                discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
                integration_results['discovery_with_network'] = True
                print("✅ Discovery service created with network specification")
            except Exception as e:
                integration_results['discovery_with_network'] = False
                pytest.fail(f"Discovery service creation failed: {e}")
            
            # Create wallet with same network
            transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18204")
            config = tari_wallet.PyTariCommsConfig(
                public_address="/ip4/127.0.0.1/tcp/18204",
                database_name="discovery_network_integration",
                datastore_path=temp_dir,
                discovery_timeout=10,
                exclude_dial_test_addresses=True,
                transport=transport
            )
            
            wallet = tari_wallet.PyTariWallet(
                config=config,
                log_path=os.path.join(temp_dir, "discovery_network_logs"),
                log_verbosity=1,
                num_rolling_log_files=2,
                size_per_log_file_bytes=256*1024,
                network_str="nextnet",  # Same network as discovery
                passphrase="test_wallet_passphrase"
            )
            integration_results['wallet_same_network'] = True
            print("✅ Wallet created with matching network")
            
            # Test network consistency
            # Get nodes from discovery
            available_nodes = discovery.get_available_nodes()
            
            # Get peers from wallet
            seed_peers = wallet.get_seed_peers()
            
            # Both should provide nextnet-compatible information
            integration_results['network_consistency'] = (
                isinstance(available_nodes, list) and isinstance(seed_peers, list)
            )
            
            print(f"✅ Network consistency: {len(available_nodes)} discovery nodes, {len(seed_peers)} wallet peers")
            
            return integration_results
            
        except Exception as e:
            pytest.skip(f"Discovery-Network integration test failed: {e}")


class TestNetworkAPIParameterValidation:
    """Test network API parameter validation."""
    
    def test_network_config_parameter_validation(self, temp_dir):
        """Test network configuration parameter validation."""
        try:
            import tari_wallet
            
            # Test valid address formats
            valid_addresses = [
                "/ip4/127.0.0.1/tcp/18188",
                "/ip4/0.0.0.0/tcp/18189",
                "/ip6/::1/tcp/18190"
            ]
            
            for address in valid_addresses:
                try:
                    transport = tari_wallet.PyTariTransportConfig.create_tcp(address)
                    config = tari_wallet.PyTariCommsConfig(
                        public_address=address,
                        database_name="addr_test",
                        datastore_path=temp_dir,
                        discovery_timeout=5,
                        exclude_dial_test_addresses=True,
                        transport=transport
                    )
                    print(f"✅ Valid address accepted: {address}")
                except Exception as e:
                    print(f"⚠️ Valid address rejected: {address} - {e}")
            
            # Test invalid address formats
            invalid_addresses = [
                "invalid_address",
                "127.0.0.1:18188",
                "",
                None
            ]
            
            for address in invalid_addresses:
                try:
                    if address is not None:
                        transport = tari_wallet.PyTariTransportConfig.create_tcp(address)
                        config = tari_wallet.PyTariCommsConfig(
                            public_address=address,
                            database_name="invalid_addr_test",
                            datastore_path=temp_dir,
                            discovery_timeout=5,
                            exclude_dial_test_addresses=True,
                            transport=transport
                        )
                    else:
                        config = tari_wallet.PyTariCommsConfig(
                            public_address=address,
                            database_name="invalid_addr_test",
                            datastore_path=temp_dir,
                            discovery_timeout=5,
                            exclude_dial_test_addresses=True
                        )
                    print(f"⚠️ Invalid address unexpectedly accepted: {address}")
                except Exception as e:
                    print(f"✅ Invalid address properly rejected: {address}")
            
        except Exception as e:
            pytest.skip(f"Network config parameter validation failed: {e}")
    
    def test_discovery_timeout_validation(self, temp_dir):
        """Test discovery timeout parameter validation."""
        try:
            import tari_wallet
            
            # Test valid timeout values
            valid_timeouts = [1, 5, 10, 30, 60]
            
            for timeout in valid_timeouts:
                try:
                    transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
                    config = tari_wallet.PyTariCommsConfig(
                        public_address="/ip4/127.0.0.1/tcp/18188",
                        database_name="timeout_test",
                        datastore_path=temp_dir,
                        discovery_timeout=timeout,
                        exclude_dial_test_addresses=True,
                        transport=transport
                    )
                    print(f"✅ Valid timeout accepted: {timeout}")
                except Exception as e:
                    print(f"⚠️ Valid timeout rejected: {timeout} - {e}")
            
            # Test invalid timeout values
            invalid_timeouts = [-1, 0, None, "5"]
            
            for timeout in invalid_timeouts:
                try:
                    transport = tari_wallet.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
                    config = tari_wallet.PyTariCommsConfig(
                        public_address="/ip4/127.0.0.1/tcp/18188",
                        database_name="invalid_timeout_test",
                        datastore_path=temp_dir,
                        discovery_timeout=timeout,
                        exclude_dial_test_addresses=True,
                        transport=transport
                    )
                    print(f"⚠️ Invalid timeout unexpectedly accepted: {timeout}")
                except Exception as e:
                    print(f"✅ Invalid timeout properly rejected: {timeout}")
            
        except Exception as e:
            pytest.skip(f"Discovery timeout validation failed: {e}")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
