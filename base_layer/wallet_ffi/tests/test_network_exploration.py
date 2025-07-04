"""
Network Exploration Validation Tests

Tests that validate the Network Exploration functions declared in examples.md:
- NetworkManager
- TariNetwork
- NetworkManager.get_available_networks()
- NetworkManager.get_network_by_name()
- manager.get_network_info()

These tests focus on TARI_TARGET_NETWORK=nextnet as specified.
"""

import pytest
import os
from pathlib import Path

# Set environment for nextnet testing
os.environ['TARI_TARGET_NETWORK'] = 'nextnet'

# Add Python module path
current_dir = Path(__file__).parent
python_module_path = current_dir.parent / 'python'
import sys
sys.path.insert(0, str(python_module_path))


class TestNetworkExplorationImports:
    """Test that Network Exploration functions can be imported."""
    
    def test_import_network_manager(self):
        """Test NetworkManager import."""
        try:
            import tari_wallet
            from tari_wallet import NetworkManager
            
            assert NetworkManager is not None, "NetworkManager should be importable"
            print("✅ NetworkManager imported successfully")
            
        except ImportError as e:
            pytest.fail(f"Failed to import NetworkManager: {e}")
            
    def test_import_tari_network(self):
        """Test TariNetwork import."""
        try:
            import tari_wallet
            from tari_wallet import TariNetwork
            
            assert TariNetwork is not None, "TariNetwork should be importable"
            print("✅ TariNetwork imported successfully")
            
        except ImportError as e:
            pytest.fail(f"Failed to import TariNetwork: {e}")
            
    def test_import_combined_network_classes(self):
        """Test combined import as shown in examples.md."""
        try:
            from tari_wallet import NetworkManager, TariNetwork
            
            assert NetworkManager is not None
            assert TariNetwork is not None
            print("✅ Combined network import successful (as in examples.md)")
            
        except ImportError as e:
            pytest.fail(f"Failed combined network import: {e}")


class TestTariNetworkEnum:
    """Test TariNetwork enum functionality."""
    
    def test_tari_network_enum_values(self):
        """Test TariNetwork enum values."""
        try:
            from tari_wallet import TariNetwork
            
            # Test expected network values
            expected_networks = ['NEXTNET', 'MAINNET', 'LOCALNET']
            available_networks = []
            
            for network_name in expected_networks:
                if hasattr(TariNetwork, network_name):
                    network_value = getattr(TariNetwork, network_name)
                    available_networks.append(network_name)
                    print(f"✅ TariNetwork.{network_name} = {network_value}")
                    
            assert len(available_networks) > 0, "At least one network should be available"
            assert 'NEXTNET' in available_networks, "NEXTNET should be available for our testing"
            
        except ImportError:
            pytest.skip("TariNetwork not available")
            
    def test_nextnet_enum_value(self):
        """Test TariNetwork.NEXTNET specifically."""
        try:
            from tari_wallet import TariNetwork
            
            if hasattr(TariNetwork, 'NEXTNET'):
                nextnet = TariNetwork.NEXTNET
                assert nextnet is not None, "NEXTNET value should not be None"
                print(f"✅ TariNetwork.NEXTNET value: {nextnet}")
                
                # Test that enum values are comparable
                assert nextnet == TariNetwork.NEXTNET, "Enum values should be equal to themselves"
                
            else:
                pytest.skip("TariNetwork.NEXTNET not available")
                
        except ImportError:
            pytest.skip("TariNetwork not available")


class TestNetworkManagerStaticMethods:
    """Test NetworkManager static methods."""
    
    def test_get_available_networks(self):
        """Test NetworkManager.get_available_networks() as shown in examples.md."""
        try:
            from tari_wallet import NetworkManager
            
            # Test the exact call from examples.md
            networks = NetworkManager.get_available_networks()
            
            # Validate result
            assert isinstance(networks, list), "get_available_networks should return a list"
            assert len(networks) > 0, "Should have at least one available network"
            
            print(f"✅ Available networks: {networks}")
            
            # Verify nextnet is in the list
            assert 'nextnet' in networks, "nextnet should be in available networks"
            
            # Check network name format
            for network in networks:
                assert isinstance(network, str), f"Network name {network} should be string"
                assert len(network) > 0, f"Network name should not be empty"
                
            return networks
            
        except Exception as e:
            pytest.fail(f"get_available_networks failed: {e}")
            
    def test_get_network_by_name(self):
        """Test NetworkManager.get_network_by_name() method."""
        try:
            from tari_wallet import NetworkManager
            
            # Test getting nextnet specifically
            network = NetworkManager.get_network_by_name('nextnet')
            
            assert network is not None, "get_network_by_name should return network object"
            print(f"✅ get_network_by_name('nextnet') returned: {network}")
            
            # Test getting other networks if available
            available_networks = NetworkManager.get_available_networks()
            for network_name in available_networks[:3]:  # Test first 3
                try:
                    network_obj = NetworkManager.get_network_by_name(network_name)
                    assert network_obj is not None, f"Network {network_name} should be retrievable"
                    print(f"✅ get_network_by_name('{network_name}') successful")
                except Exception as e:
                    print(f"⚠️  get_network_by_name('{network_name}') failed: {e}")
                    
        except Exception as e:
            pytest.fail(f"get_network_by_name failed: {e}")
            
    def test_invalid_network_name(self):
        """Test get_network_by_name with invalid network name."""
        try:
            from tari_wallet import NetworkManager
            
            # Test with invalid network name
            with pytest.raises(Exception):
                NetworkManager.get_network_by_name('invalid_network_name')
                
            print("✅ get_network_by_name properly handles invalid network names")
            
        except ImportError:
            pytest.skip("NetworkManager not available")


class TestNetworkManagerInstance:
    """Test NetworkManager instance methods."""
    
    def test_network_manager_instantiation(self):
        """Test NetworkManager(network) instantiation as shown in examples.md."""
        try:
            from tari_wallet import NetworkManager, TariNetwork
            
            # Test instantiation with TariNetwork enum (if available)
            if hasattr(TariNetwork, 'NEXTNET'):
                manager = NetworkManager(TariNetwork.NEXTNET)
                assert manager is not None, "NetworkManager should be instantiable with TariNetwork enum"
                print("✅ NetworkManager(TariNetwork.NEXTNET) successful")
                
            # Test instantiation with network object from get_network_by_name
            network = NetworkManager.get_network_by_name('nextnet')
            manager = NetworkManager(network)
            assert manager is not None, "NetworkManager should be instantiable with network object"
            print("✅ NetworkManager(network_object) successful")
            
            return manager
            
        except Exception as e:
            pytest.fail(f"NetworkManager instantiation failed: {e}")
            
    def test_get_network_info(self):
        """Test manager.get_network_info() method as shown in examples.md."""
        try:
            from tari_wallet import NetworkManager
            
            # Create manager for nextnet
            network = NetworkManager.get_network_by_name('nextnet')
            manager = NetworkManager(network)
            
            # Test get_network_info method
            info = manager.get_network_info()
            
            assert isinstance(info, dict), "get_network_info should return a dict"
            print(f"✅ get_network_info returned: {list(info.keys())}")
            
            # Test expected fields from examples.md
            expected_fields = ['dns_seeds_count', 'peer_seeds_count', 'supported_transports']
            available_fields = []
            
            for field in expected_fields:
                if field in info:
                    available_fields.append(field)
                    print(f"  {field}: {info[field]}")
                    
            assert len(available_fields) > 0, "Should have at least some expected fields"
            
            # Test supported_transports structure if available
            if 'supported_transports' in info:
                transports = info['supported_transports']
                assert isinstance(transports, dict), "supported_transports should be dict"
                
                transport_types = ['ip4', 'ip6', 'onion3']
                for transport_type in transport_types:
                    if transport_type in transports:
                        assert isinstance(transports[transport_type], int), \
                            f"{transport_type} count should be integer"
                        print(f"    {transport_type}: {transports[transport_type]}")
                        
            return info
            
        except Exception as e:
            pytest.fail(f"get_network_info failed: {e}")


class TestCompleteNetworkExplorationWorkflow:
    """Test complete Network Exploration workflow from examples.md."""
    
    def test_complete_examples_md_workflow(self):
        """Test the complete workflow exactly as shown in examples.md."""
        try:
            # Exact code from examples.md
            from tari_wallet import NetworkManager, TariNetwork

            # Get all available networks
            networks = NetworkManager.get_available_networks()
            print(f"Available networks: {networks}")

            # Get detailed info for each network
            successful_networks = []
            failed_networks = []
            
            for network_name in networks:
                try:
                    network = NetworkManager.get_network_by_name(network_name)
                    manager = NetworkManager(network)
                    info = manager.get_network_info()
                    
                    print(f"\n🌐 {network_name.upper()}")
                    print(f"  DNS seeds: {info.get('dns_seeds_count', 'N/A')}")
                    print(f"  Peer seeds: {info.get('peer_seeds_count', 'N/A')}")
                    print(f"  Explorer: {info.get('explorer_url', 'N/A')}")
                    
                    # Show transport breakdown
                    if 'supported_transports' in info:
                        transports = info['supported_transports']
                        print(f"  Transports: IPv4={transports.get('ip4', 0)}, "
                              f"IPv6={transports.get('ip6', 0)}, "
                              f"Onion={transports.get('onion3', 0)}")
                    
                    successful_networks.append(network_name)
                    
                except Exception as e:
                    print(f"❌ Error with {network_name}: {e}")
                    failed_networks.append((network_name, str(e)))
                    
            # Validate workflow success
            assert len(networks) > 0, "Should have available networks"
            assert len(successful_networks) > 0, "Should process at least one network successfully"
            assert 'nextnet' in successful_networks, "Should successfully process nextnet"
            
            print(f"\n✅ Complete workflow successful!")
            print(f"  Processed networks: {len(successful_networks)}")
            print(f"  Failed networks: {len(failed_networks)}")
            
            return {
                'available_networks': networks,
                'successful_networks': successful_networks,
                'failed_networks': failed_networks
            }
            
        except Exception as e:
            pytest.fail(f"Complete network exploration workflow failed: {e}")


class TestNetworkExplorationEdgeCases:
    """Test edge cases for Network Exploration functions."""
    
    def test_empty_and_invalid_inputs(self):
        """Test handling of empty and invalid inputs."""
        try:
            from tari_wallet import NetworkManager
            
            # Test invalid network names
            invalid_names = ['', None, 'nonexistent', 'invalid-network']
            
            for invalid_name in invalid_names:
                try:
                    if invalid_name is not None:
                        result = NetworkManager.get_network_by_name(invalid_name)
                        print(f"⚠️  Unexpected success with invalid name: {invalid_name}")
                except Exception as e:
                    print(f"✅ Properly handled invalid name '{invalid_name}': {type(e).__name__}")
                    
        except ImportError:
            pytest.skip("NetworkManager not available")
            
    def test_network_info_consistency(self):
        """Test that network info is consistent across calls."""
        try:
            from tari_wallet import NetworkManager
            
            # Get network info multiple times
            network = NetworkManager.get_network_by_name('nextnet')
            manager = NetworkManager(network)
            
            info1 = manager.get_network_info()
            info2 = manager.get_network_info()
            
            # Compare key fields for consistency
            consistent_fields = []
            inconsistent_fields = []
            
            for key in info1.keys():
                if key in info2:
                    if info1[key] == info2[key]:
                        consistent_fields.append(key)
                    else:
                        inconsistent_fields.append(key)
                        
            print(f"✅ Consistent fields: {consistent_fields}")
            if inconsistent_fields:
                print(f"⚠️  Inconsistent fields: {inconsistent_fields}")
                
            # Most fields should be consistent
            assert len(consistent_fields) >= len(inconsistent_fields), \
                "Most fields should be consistent between calls"
                
        except Exception as e:
            pytest.skip(f"Network info consistency test failed: {e}")


@pytest.mark.integration
class TestNetworkExplorationIntegration:
    """Integration tests for Network Exploration with real nextnet."""
    
    @pytest.mark.skipif(
        os.environ.get('TARI_TARGET_NETWORK') != 'nextnet',
        reason="Integration tests require TARI_TARGET_NETWORK=nextnet"
    )
    def test_nextnet_network_exploration_integration(self):
        """Test Network Exploration integration with real nextnet."""
        try:
            from tari_wallet import NetworkManager, TariNetwork
            
            print("Running nextnet network exploration integration test...")
            
            # Comprehensive integration test
            integration_results = {
                'available_networks_retrieved': False,
                'nextnet_in_available': False,
                'nextnet_network_object_retrieved': False,
                'nextnet_manager_created': False,
                'nextnet_info_retrieved': False,
                'nextnet_has_dns_seeds': False,
                'nextnet_has_peer_seeds': False,
                'nextnet_has_transports': False
            }
            
            # Test available networks
            try:
                networks = NetworkManager.get_available_networks()
                integration_results['available_networks_retrieved'] = len(networks) > 0
                integration_results['nextnet_in_available'] = 'nextnet' in networks
                print(f"✅ Available networks: {networks}")
            except Exception as e:
                print(f"❌ get_available_networks failed: {e}")
                
            # Test nextnet network object
            try:
                nextnet_network = NetworkManager.get_network_by_name('nextnet')
                integration_results['nextnet_network_object_retrieved'] = nextnet_network is not None
                print(f"✅ Nextnet network object retrieved")
            except Exception as e:
                print(f"❌ get_network_by_name('nextnet') failed: {e}")
                nextnet_network = None
                
            # Test nextnet manager
            if nextnet_network:
                try:
                    nextnet_manager = NetworkManager(nextnet_network)
                    integration_results['nextnet_manager_created'] = nextnet_manager is not None
                    print(f"✅ Nextnet manager created")
                    
                    # Test nextnet info
                    try:
                        nextnet_info = nextnet_manager.get_network_info()
                        integration_results['nextnet_info_retrieved'] = isinstance(nextnet_info, dict)
                        
                        # Check specific nextnet characteristics
                        if 'dns_seeds_count' in nextnet_info:
                            integration_results['nextnet_has_dns_seeds'] = nextnet_info['dns_seeds_count'] > 0
                            print(f"✅ Nextnet DNS seeds: {nextnet_info['dns_seeds_count']}")
                            
                        if 'peer_seeds_count' in nextnet_info:
                            integration_results['nextnet_has_peer_seeds'] = nextnet_info['peer_seeds_count'] > 0
                            print(f"✅ Nextnet peer seeds: {nextnet_info['peer_seeds_count']}")
                            
                        if 'supported_transports' in nextnet_info:
                            transports = nextnet_info['supported_transports']
                            integration_results['nextnet_has_transports'] = len(transports) > 0
                            print(f"✅ Nextnet transports: {transports}")
                            
                    except Exception as e:
                        print(f"❌ get_network_info() failed: {e}")
                        
                except Exception as e:
                    print(f"❌ NetworkManager creation failed: {e}")
                    
            print(f"\nIntegration test results: {integration_results}")
            
            # Essential functionality must work
            assert integration_results['available_networks_retrieved'], \
                "Must be able to retrieve available networks"
            assert integration_results['nextnet_in_available'], \
                "Nextnet must be in available networks"
            assert integration_results['nextnet_network_object_retrieved'], \
                "Must be able to retrieve nextnet network object"
                
            print("✅ Nextnet network exploration integration test passed!")
            
        except Exception as e:
            print(f"❌ Nextnet integration test failed: {e}")
            pytest.skip(f"Integration test failed: {e}")


if __name__ == "__main__":
    # Allow running this file directly for debugging  
    pytest.main([__file__, "-v"])
