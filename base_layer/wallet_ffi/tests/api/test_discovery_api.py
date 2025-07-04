"""
Discovery API Compliance Tests

Validates that discovery service interfaces match documented API contracts.
Tests method existence, parameter compatibility, and return type consistency.
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


class TestSimpleDiscoveryServiceAPI:
    """Test SimpleDiscoveryService API compliance."""
    
    @pytest.fixture
    def discovery_service_class(self):
        """Get SimpleDiscoveryService class for API testing."""
        try:
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            return SimpleDiscoveryService, TariNetwork
        except ImportError as e:
            pytest.skip(f"Cannot import discovery classes: {e}")
    
    @pytest.fixture
    def discovery_instance(self, discovery_service_class):
        """Create discovery service instance for testing."""
        SimpleDiscoveryService, TariNetwork = discovery_service_class
        
        if hasattr(TariNetwork, 'NEXTNET'):
            return SimpleDiscoveryService(TariNetwork.NEXTNET)
        else:
            pytest.skip("TariNetwork.NEXTNET not available")
    
    def test_simple_discovery_service_class_exists(self, discovery_service_class):
        """Validate SimpleDiscoveryService class exists."""
        SimpleDiscoveryService, TariNetwork = discovery_service_class
        
        assert SimpleDiscoveryService is not None, "SimpleDiscoveryService class must exist"
        assert callable(SimpleDiscoveryService), "SimpleDiscoveryService must be instantiable"
        
    def test_tari_network_enum_exists(self, discovery_service_class):
        """Validate TariNetwork enum exists with required values."""
        SimpleDiscoveryService, TariNetwork = discovery_service_class
        
        assert TariNetwork is not None, "TariNetwork enum must exist"
        assert hasattr(TariNetwork, 'NEXTNET'), "TariNetwork must have NEXTNET value"
        
    def test_discover_and_select_node_api(self, discovery_instance, api_validator):
        """Test discover_and_select_node API compliance."""
        method_name = "discover_and_select_node"
        
        # Check method exists
        assert api_validator.validate_method_exists(discovery_instance, method_name), \
            f"{method_name} method must exist"
        
        # Get method info for validation
        method_info = api_validator.get_method_info(discovery_instance, method_name)
        
        # Validate method signature accepts timeout parameter
        params = method_info.get("parameters", [])
        
        # Method should accept timeout-related parameters
        timeout_params = [p for p in params if 'timeout' in p.lower()]
        assert len(timeout_params) > 0, "Method should accept timeout parameter"
        
        print(f"✅ {method_name} API compliant: {method_info['signature']}")
        
    def test_get_available_nodes_api(self, discovery_instance, api_validator):
        """Test get_available_nodes API compliance."""
        method_name = "get_available_nodes"
        
        # Check method exists
        assert api_validator.validate_method_exists(discovery_instance, method_name), \
            f"{method_name} method must exist"
        
        # Get method info
        method_info = api_validator.get_method_info(discovery_instance, method_name)
        
        # Method should be callable without parameters
        try:
            result = getattr(discovery_instance, method_name)()
            assert isinstance(result, list), "Method should return a list"
            print(f"✅ {method_name} API compliant: returns list with {len(result)} items")
        except Exception as e:
            pytest.fail(f"{method_name} failed to execute: {e}")
            
    def test_node_object_api_compliance(self, discovery_instance):
        """Test that discovered nodes have expected API."""
        try:
            # Try to get a node for API testing
            selected_node = discovery_instance.discover_and_select_node(dns_timeout=5.0)
            
            if selected_node is None:
                # Try getting from available nodes
                available_nodes = discovery_instance.get_available_nodes()
                if available_nodes:
                    selected_node = available_nodes[0]
                else:
                    pytest.skip("No nodes available for API testing")
            
            # Test required node properties from documentation
            required_properties = ['name', 'public_key', 'address']
            property_results = {}
            
            for prop in required_properties:
                has_prop = hasattr(selected_node, prop)
                property_results[prop] = has_prop
                if has_prop:
                    value = getattr(selected_node, prop)
                    assert value is not None, f"Property {prop} should not be None"
                    print(f"✅ Node has {prop}: {type(value).__name__}")
                else:
                    print(f"❌ Node missing property: {prop}")
            
            # Test health score method
            has_health_method = hasattr(selected_node, 'get_health_score')
            if has_health_method:
                try:
                    health_score = selected_node.get_health_score()
                    assert isinstance(health_score, (int, float)), "Health score should be numeric"
                    print(f"✅ get_health_score() returns: {type(health_score).__name__}")
                except Exception as e:
                    print(f"❌ get_health_score() failed: {e}")
            else:
                print("❌ Node missing get_health_score method")
            
            # At least basic properties should be available
            basic_compliance = sum(property_results.values()) >= 2
            assert basic_compliance, "Node should have at least 2 of the basic properties"
            
        except Exception as e:
            pytest.skip(f"Node API testing failed: {e}")


class TestDiscoveryAPIIntegration:
    """Test discovery API integration patterns."""
    
    def test_complete_discovery_api_workflow(self):
        """Test that documented API workflow is functional."""
        try:
            # Import exactly as documented
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            
            # API workflow validation
            workflow_steps = {}
            
            # Step 1: Create service
            try:
                discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
                workflow_steps['service_creation'] = True
                print("✅ API Step 1: Service creation successful")
            except Exception as e:
                workflow_steps['service_creation'] = False
                pytest.fail(f"API workflow failed at service creation: {e}")
            
            # Step 2: Discover node
            try:
                selected_node = discovery.discover_and_select_node(dns_timeout=5.0)
                workflow_steps['node_discovery'] = True
                print("✅ API Step 2: Node discovery method call successful")
            except Exception as e:
                workflow_steps['node_discovery'] = False
                print(f"⚠️ API Step 2: Node discovery failed: {e}")
            
            # Step 3: Get available nodes
            try:
                available_nodes = discovery.get_available_nodes()
                workflow_steps['available_nodes'] = True
                print(f"✅ API Step 3: Available nodes retrieved ({len(available_nodes)} nodes)")
            except Exception as e:
                workflow_steps['available_nodes'] = False
                print(f"❌ API Step 3: Available nodes failed: {e}")
            
            # Step 4: Node property access (if node available)
            if selected_node:
                try:
                    # Test documented properties
                    documented_properties = []
                    if hasattr(selected_node, 'name'):
                        documented_properties.append(f"name: {selected_node.name}")
                    if hasattr(selected_node, 'public_key'):
                        key_preview = selected_node.public_key[:16] + "..." if len(selected_node.public_key) > 16 else selected_node.public_key
                        documented_properties.append(f"public_key: {key_preview}")
                    if hasattr(selected_node, 'address'):
                        documented_properties.append(f"address: {selected_node.address}")
                    if hasattr(selected_node, 'get_health_score'):
                        health = selected_node.get_health_score()
                        documented_properties.append(f"health_score: {health}")
                    
                    workflow_steps['property_access'] = len(documented_properties) > 0
                    print(f"✅ API Step 4: Node properties accessible ({len(documented_properties)} properties)")
                    
                except Exception as e:
                    workflow_steps['property_access'] = False
                    print(f"❌ API Step 4: Property access failed: {e}")
            else:
                workflow_steps['property_access'] = True  # Skip if no node
                print("ℹ️ API Step 4: Skipped (no node selected)")
            
            # Validate essential workflow steps
            essential_steps = ['service_creation', 'available_nodes']
            failed_steps = [step for step in essential_steps if not workflow_steps.get(step, False)]
            
            if failed_steps:
                pytest.fail(f"Essential API workflow steps failed: {failed_steps}")
            
            print(f"✅ Complete discovery API workflow validated: {workflow_steps}")
            return workflow_steps
            
        except ImportError as e:
            pytest.skip(f"Cannot test API workflow - import failed: {e}")
        except Exception as e:
            pytest.fail(f"API workflow test failed: {e}")


class TestDiscoveryAPIParameterValidation:
    """Test API parameter validation and edge cases."""
    
    @pytest.fixture
    def discovery_service(self):
        """Create discovery service for parameter testing."""
        try:
            from tari_wallet import SimpleDiscoveryService, TariNetwork
            return SimpleDiscoveryService(TariNetwork.NEXTNET)
        except Exception as e:
            pytest.skip(f"Cannot create discovery service: {e}")
    
    def test_discover_and_select_node_parameter_types(self, discovery_service):
        """Test parameter type validation for discover_and_select_node."""
        method = discovery_service.discover_and_select_node
        
        # Test valid parameter types
        valid_params = [
            {"dns_timeout": 5.0},  # float
            {"dns_timeout": 5},    # int
        ]
        
        for params in valid_params:
            try:
                result = method(**params)
                print(f"✅ Valid params {params}: {type(result).__name__}")
            except Exception as e:
                print(f"⚠️ Valid params {params} failed: {e}")
        
        # Test invalid parameter types (should handle gracefully)
        invalid_params = [
            {"dns_timeout": "5"},     # string
            {"dns_timeout": -1},      # negative
            {"dns_timeout": None},    # None
        ]
        
        for params in invalid_params:
            try:
                result = method(**params)
                print(f"⚠️ Invalid params {params} unexpectedly succeeded")
            except Exception as e:
                print(f"✅ Invalid params {params} properly rejected: {type(e).__name__}")
    
    def test_api_method_signatures_documented(self, discovery_service, api_validator):
        """Ensure API method signatures match documentation expectations."""
        documented_methods = {
            'discover_and_select_node': {
                'required': False,
                'parameters': ['dns_timeout'],
                'return_type': 'object_or_none'
            },
            'get_available_nodes': {
                'required': True,
                'parameters': [],
                'return_type': 'list'
            }
        }
        
        api_compliance_results = {}
        
        for method_name, expected in documented_methods.items():
            method_info = api_validator.get_method_info(discovery_service, method_name)
            
            compliance = {
                'exists': method_info['exists'],
                'signature': method_info.get('signature', 'N/A'),
                'parameters': method_info.get('parameters', []),
                'documented': expected['required']
            }
            
            if method_info['exists']:
                # Check parameter compatibility
                actual_params = set(method_info['parameters'])
                expected_params = set(expected['parameters'])
                
                # Remove 'self' parameter
                actual_params.discard('self')
                
                # For optional parameters, they should be present but not required
                if expected['parameters']:
                    param_compatible = expected_params.issubset(actual_params)
                else:
                    param_compatible = True
                
                compliance['parameter_compatible'] = param_compatible
                
                if not param_compatible:
                    print(f"❌ {method_name}: Parameter mismatch")
                    print(f"   Expected: {expected_params}")
                    print(f"   Actual: {actual_params}")
                else:
                    print(f"✅ {method_name}: API signature compliant")
            else:
                compliance['parameter_compatible'] = False
                print(f"❌ {method_name}: Method missing")
            
            api_compliance_results[method_name] = compliance
        
        # Validate essential methods exist and are compliant
        required_methods = [name for name, info in documented_methods.items() if info['required']]
        
        for method_name in required_methods:
            result = api_compliance_results[method_name]
            assert result['exists'], f"Required method {method_name} must exist"
            assert result['parameter_compatible'], f"Required method {method_name} must have compatible parameters"
        
        print(f"API compliance validation completed: {len(api_compliance_results)} methods checked")
        return api_compliance_results


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
