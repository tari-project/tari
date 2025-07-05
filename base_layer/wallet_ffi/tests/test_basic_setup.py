"""
Basic setup and smoke tests for Tari wallet Python bindings.

This module contains fundamental tests to ensure the development environment
is properly configured and basic functionality works.
"""

import sys
import os
import pytest
import importlib.util
from pathlib import Path


class TestBasicSetup:
    """Test basic development environment setup."""
    
    def test_python_version(self):
        """Test that Python version is supported."""
        assert sys.version_info >= (3, 8), "Python 3.8+ is required"
        assert sys.version_info < (4, 0), "Python 4.0+ is not yet supported"
        
    def test_python_path_setup(self):
        """Test that Python path is properly configured."""
        # Check that we can access the current directory
        current_dir = Path(__file__).parent
        assert current_dir.exists(), "Current directory should exist"
        
        # Check that parent directory (wallet_ffi) exists
        wallet_ffi_dir = current_dir.parent
        assert wallet_ffi_dir.exists(), "wallet_ffi directory should exist"
        
    def test_import_basic_modules(self):
        """Test that basic Python modules can be imported."""
        import asyncio
        import json
        import logging
        import threading
        import time
        
        # Test that modules are accessible
        assert hasattr(asyncio, 'run'), "asyncio should have run function"
        assert hasattr(json, 'loads'), "json should have loads function"
        assert hasattr(logging, 'getLogger'), "logging should have getLogger function"
        
    def test_async_functionality(self):
        """Test basic async functionality."""
        import asyncio
        
        async def async_test():
            await asyncio.sleep(0.01)
            return "async_works"
        
        result = asyncio.run(async_test())
        assert result == "async_works", "Async functionality should work"
        
    def test_environment_variables(self):
        """Test that environment variables are properly set."""
        # Test TARI_TARGET_NETWORK if it exists
        target_network = os.environ.get('TARI_TARGET_NETWORK')
        if target_network:
            assert target_network in ['mainnet', 'testnet', 'nextnet'], \
                f"Invalid TARI_TARGET_NETWORK: {target_network}"
        
    def test_file_permissions(self):
        """Test that file permissions are correct."""
        test_file = Path(__file__)
        assert test_file.exists(), "Test file should exist"
        assert test_file.is_file(), "Test file should be a file"
        
        # Check that we can read the file
        with open(test_file, 'r') as f:
            content = f.read()
            assert len(content) > 0, "Test file should have content"
            
    def test_directory_structure(self):
        """Test that expected directory structure exists."""
        base_dir = Path(__file__).parent.parent
        
        # Check for key directories
        expected_dirs = [
            'src',
            'python',
            'tests',
            'scripts',
        ]
        
        for dir_name in expected_dirs:
            dir_path = base_dir / dir_name
            assert dir_path.exists(), f"Directory {dir_name} should exist"
            assert dir_path.is_dir(), f"{dir_name} should be a directory"
            
    def test_configuration_files(self):
        """Test that configuration files exist."""
        base_dir = Path(__file__).parent.parent
        
        # Check for key configuration files
        expected_files = [
            'Cargo.toml',
            'pyproject.toml',
            'requirements-dev.txt',
        ]
        
        for file_name in expected_files:
            file_path = base_dir / file_name
            assert file_path.exists(), f"Configuration file {file_name} should exist"
            assert file_path.is_file(), f"{file_name} should be a file"
            
    def test_build_artifacts(self):
        """Test for build artifacts and build configuration."""
        base_dir = Path(__file__).parent.parent
        
        # Check for build.rs
        build_rs = base_dir / 'build.rs'
        assert build_rs.exists(), "build.rs should exist"
        
        # Check for .cargo directory
        cargo_dir = base_dir / '.cargo'
        if cargo_dir.exists():
            config_toml = cargo_dir / 'config.toml'
            assert config_toml.exists(), ".cargo/config.toml should exist if .cargo directory exists"
            
    def test_logging_setup(self):
        """Test that logging can be configured."""
        import logging
        
        # Create a test logger
        logger = logging.getLogger('test_logger')
        logger.setLevel(logging.DEBUG)
        
        # Create a handler
        handler = logging.StreamHandler()
        handler.setLevel(logging.DEBUG)
        
        # Create formatter
        formatter = logging.Formatter('%(asctime)s - %(name)s - %(levelname)s - %(message)s')
        handler.setFormatter(formatter)
        
        # Add handler to logger
        logger.addHandler(handler)
        
        # Test logging
        logger.info("Test log message")
        
        # Clean up
        logger.removeHandler(handler)
        
    def test_threading_support(self):
        """Test basic threading functionality."""
        import threading
        import time
        
        result = []
        
        def worker():
            time.sleep(0.01)
            result.append("thread_completed")
        
        thread = threading.Thread(target=worker)
        thread.start()
        thread.join(timeout=1.0)
        
        assert not thread.is_alive(), "Thread should complete"
        assert len(result) == 1, "Thread should have completed work"
        assert result[0] == "thread_completed", "Thread should have correct result"


class TestDevelopmentDependencies:
    """Test that development dependencies are available."""
    
    def test_pytest_available(self):
        """Test that pytest is available."""
        import pytest
        assert hasattr(pytest, 'main'), "pytest should have main function"
        
    def test_pytest_asyncio_available(self):
        """Test that pytest-asyncio is available."""
        try:
            import pytest_asyncio
            assert pytest_asyncio is not None, "pytest-asyncio should be importable"
        except ImportError:
            pytest.skip("pytest-asyncio not installed")
            
    def test_basic_assertions(self):
        """Test basic assertion functionality."""
        assert True is True
        assert False is False
        assert 1 == 1
        assert "test" == "test"
        assert [1, 2, 3] == [1, 2, 3]
        
    def test_error_handling(self):
        """Test error handling mechanisms."""
        with pytest.raises(ValueError):
            raise ValueError("Test error")
            
        with pytest.raises(TypeError):
            raise TypeError("Test type error")
            
    def test_parametrized_tests(self):
        """Test parametrized test functionality."""
        @pytest.mark.parametrize("input_value,expected", [
            (1, 1),
            (2, 2),
            ("test", "test"),
        ])
        def check_identity(input_value, expected):
            assert input_value == expected
            
        # Run the parametrized test
        check_identity(1, 1)
        check_identity(2, 2)
        check_identity("test", "test")


@pytest.mark.asyncio
async def test_async_test_support():
    """Test async test support."""
    import asyncio
    
    async def async_operation():
        await asyncio.sleep(0.01)
        return "async_test_complete"
    
    result = await async_operation()
    assert result == "async_test_complete"


if __name__ == "__main__":
    # Run tests if script is executed directly
    pytest.main([__file__, "-v"])
