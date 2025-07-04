"""
API-specific test fixtures for Tari wallet Python bindings.

Extends parent conftest.py with API validation specific fixtures
for contract testing and interface validation.
"""

import pytest
import inspect
import types
from typing import Dict, List, Any, Callable
from dataclasses import dataclass


@dataclass
class APITestCase:
    """Definition of an API test case for validation."""
    method_name: str
    expected_params: List[str]
    expected_return_type: type
    test_data: Dict[str, Any]


@dataclass  
class ValidationResult:
    """Result of API compliance validation."""
    api_compliant: bool
    missing_methods: List[str]
    type_mismatches: List[str]
    param_mismatches: List[str]


@pytest.fixture
def api_validator():
    """Create an API validation helper."""
    class APIValidator:
        def __init__(self):
            self.validation_errors = []
        
        def validate_method_exists(self, obj: Any, method_name: str) -> bool:
            """Check if method exists on object."""
            return hasattr(obj, method_name) and callable(getattr(obj, method_name))
        
        def validate_method_signature(self, obj: Any, method_name: str, expected_params: List[str]) -> bool:
            """Validate method signature matches expected parameters."""
            if not self.validate_method_exists(obj, method_name):
                return False
            
            method = getattr(obj, method_name)
            sig = inspect.signature(method)
            actual_params = list(sig.parameters.keys())
            
            # Remove 'self' parameter for instance methods
            if actual_params and actual_params[0] == 'self':
                actual_params = actual_params[1:]
            
            return set(expected_params).issubset(set(actual_params))
        
        def validate_return_type(self, method_result: Any, expected_type: type) -> bool:
            """Validate method return type."""
            if expected_type is None:
                return method_result is None
            return isinstance(method_result, expected_type)
        
        def get_method_info(self, obj: Any, method_name: str) -> Dict[str, Any]:
            """Get detailed method information for debugging."""
            if not self.validate_method_exists(obj, method_name):
                return {"exists": False}
            
            method = getattr(obj, method_name)
            sig = inspect.signature(method)
            
            return {
                "exists": True,
                "signature": str(sig),
                "parameters": list(sig.parameters.keys()),
                "return_annotation": sig.return_annotation,
                "docstring": inspect.getdoc(method)
            }
    
    return APIValidator()


@pytest.fixture
def discovery_api_tests():
    """API test cases for discovery service validation."""
    return [
        APITestCase(
            method_name="discover_and_select_node",
            expected_params=["timeout"],
            expected_return_type=object,  # Node object
            test_data={"timeout": 30}
        ),
        APITestCase(
            method_name="get_available_nodes", 
            expected_params=[],
            expected_return_type=list,
            test_data={}
        ),
        APITestCase(
            method_name="get_health_score",
            expected_params=[],
            expected_return_type=(int, float),
            test_data={}
        )
    ]


@pytest.fixture
def wallet_api_tests():
    """API test cases for wallet interface validation."""
    return [
        APITestCase(
            method_name="get_balance",
            expected_params=[],
            expected_return_type=object,  # PyTariBalance
            test_data={}
        ),
        APITestCase(
            method_name="get_seed_peers",
            expected_params=[],
            expected_return_type=list,
            test_data={}
        ),
        APITestCase(
            method_name="sign_message",
            expected_params=["message"],
            expected_return_type=str,
            test_data={"message": "test message"}
        )
    ]


@pytest.fixture
def network_api_tests():
    """API test cases for network interface validation."""
    return [
        APITestCase(
            method_name="get_base_node_info",
            expected_params=[],
            expected_return_type=dict,
            test_data={}
        ),
        APITestCase(
            method_name="format_base_node_info",
            expected_params=["node_info"],
            expected_return_type=str,
            test_data={"node_info": {"name": "test", "public_key": "test_key"}}
        )
    ]
