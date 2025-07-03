#!/usr/bin/env python3
"""
Distribution Preparation Script

This script prepares the Tari wallet core functionality for network-specific
PyPI package distribution.
"""

import os
import shutil
import json
from pathlib import Path


def create_network_package(network_name: str, base_dir: Path):
    """
    Create a network-specific package structure
    
    Args:
        network_name: Name of the network (mainnet, testnet, nextnet)
        base_dir: Base directory for package creation
    """
    package_name = f"tari_wallet_{network_name}"
    package_dir = base_dir / package_name
    
    # Create package directory
    package_dir.mkdir(exist_ok=True)
    
    # Create __init__.py with network-specific defaults
    init_content = f'''"""
Tari Wallet for {network_name.title()}

This package provides Tari wallet functionality specifically configured
for the {network_name} network.
"""

from tari_wallet_core import (
    create_network_wallet,
    get_network_defaults,
    format_base_node_info,
    refresh_base_node_list,
    set_next_base_node,
    sync_base_node,
    BaseNodeSelectionStrategy,
    PersistentNodeSelector,
    WalletSyncManager
)


def create_wallet(database_name: str = None, **kwargs):
    """
    Create a {network_name} wallet with auto-discovery
    
    Args:
        database_name: Name for wallet database
        **kwargs: Additional wallet configuration options
        
    Returns:
        Tuple of (wallet, base_node_info)
    """
    defaults = get_network_defaults("{network_name}")
    
    # Merge defaults with user kwargs
    config = {{**defaults, **kwargs}}
    
    return create_network_wallet(
        network_name="{network_name}",
        database_name=database_name or f"{network_name}_wallet",
        **config
    )


def create_wallet_with_discovery(
    database_name: str = None,
    discovery_timeout: float = 30.0,
    **kwargs
):
    """
    Create a {network_name} wallet with explicit discovery workflow
    
    Args:
        database_name: Name for wallet database
        discovery_timeout: Timeout for node discovery
        **kwargs: Additional wallet configuration options
        
    Returns:
        Tuple of (wallet, base_node_info)
    """
    return create_wallet(
        database_name=database_name,
        discovery_timeout=discovery_timeout,
        explicit_workflow=True,
        **kwargs
    )


# Network-specific convenience function
create_{network_name}_wallet = create_wallet


__all__ = [
    'create_wallet',
    'create_wallet_with_discovery',
    'create_{network_name}_wallet',
    'format_base_node_info',
    'refresh_base_node_list',
    'set_next_base_node',
    'sync_base_node',
    'BaseNodeSelectionStrategy',
    'PersistentNodeSelector',
    'WalletSyncManager',
]
'''
    
    with open(package_dir / "__init__.py", "w") as f:
        f.write(init_content)
    
    return package_dir


def create_pyproject_toml(network_name: str, package_dir: Path):
    """
    Create pyproject.toml for network-specific package
    
    Args:
        network_name: Name of the network
        package_dir: Package directory
    """
    package_name = f"tari-wallet-{network_name}"
    
    pyproject_content = f'''[build-system]
requires = ["setuptools>=61.0", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "{package_name}"
version = "0.1.0"
description = "Python bindings for Tari cryptocurrency wallet - {network_name.title()} network"
authors = [
    {{name = "The Tari Development Community", email = "dev@tari.com"}}
]
readme = "README.md"
license = {{text = "BSD-3-Clause"}}
homepage = "https://tari.com"
repository = "https://github.com/tari-project/tari"
classifiers = [
    "Development Status :: 3 - Alpha",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: BSD License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.8",
    "Programming Language :: Python :: 3.9",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Topic :: Security :: Cryptography",
    "Topic :: Software Development :: Libraries :: Python Modules",
]
requires-python = ">=3.8"
dependencies = [
    "tari-wallet-core>=0.1.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=6.0",
    "pytest-asyncio>=0.18.0",
]

[project.urls]
"Bug Reports" = "https://github.com/tari-project/tari/issues"
"Source" = "https://github.com/tari-project/tari"
"Documentation" = "https://tari.com/docs"

[tool.setuptools.packages.find]
where = ["."]
include = ["tari_wallet_{network_name}*"]

[tool.setuptools.package-data]
"tari_wallet_{network_name}" = ["*.md", "*.txt"]
'''
    
    with open(package_dir / "pyproject.toml", "w") as f:
        f.write(pyproject_content)


def create_readme(network_name: str, package_dir: Path):
    """
    Create README.md for network-specific package
    
    Args:
        network_name: Name of the network
        package_dir: Package directory
    """
    readme_content = f'''# Tari Wallet - {network_name.title()} Network

Python bindings for the Tari cryptocurrency wallet, specifically configured for the **{network_name}** network.

## Installation

```bash
pip install tari-wallet-{network_name}
```

## Quick Start

```python
from tari_wallet_{network_name} import create_wallet

# Create a wallet with automatic base node discovery
wallet, node_info = create_wallet("my_wallet")

print(f"Wallet created successfully!")
print(f"Connected to: {{node_info}}")

# Check balance
balance = wallet.get_balance()
print(f"Available balance: {{balance.available}} microTari")
```

## Explicit Discovery Workflow

For more control over the discovery process:

```python
from tari_wallet_{network_name} import (
    create_wallet_with_discovery,
    refresh_base_node_list,
    set_next_base_node,
    sync_base_node
)

# Create wallet with explicit three-step workflow
wallet, node_info = create_wallet_with_discovery(
    "my_wallet",
    discovery_timeout=30.0
)
```

## Features

- **Automatic base node discovery** for {network_name} network
- **Round-robin node selection** with persistent state
- **Peer caching** for instant subsequent connections
- **Health tracking** and automatic failover
- **Explicit workflow control** for advanced users

## Network Information

- **Network**: {network_name}
- **Auto-configured**: Yes
- **DNS Seeds**: Automatically discovered
- **Fallback Nodes**: Built-in reliable nodes

## Documentation

For detailed documentation, visit [tari.com/docs](https://tari.com/docs)

## License

This project is licensed under the BSD-3-Clause License.
'''
    
    with open(package_dir / "README.md", "w") as f:
        f.write(readme_content)


def main():
    """Main preparation function"""
    print("Preparing Tari wallet packages for PyPI distribution...")
    
    # Base directory for package creation
    base_dir = Path("dist_packages")
    base_dir.mkdir(exist_ok=True)
    
    networks = ["mainnet", "testnet", "nextnet"]
    
    for network in networks:
        print(f"Creating {network} package...")
        
        # Create package structure
        package_dir = create_network_package(network, base_dir)
        create_pyproject_toml(network, package_dir)
        create_readme(network, package_dir)
        
        print(f"  ✓ Created {package_dir}")
    
    # Create core package structure
    print("Creating core package...")
    core_dir = base_dir / "tari_wallet_core"
    core_dir.mkdir(exist_ok=True)
    
    # Copy core module
    shutil.copytree("python/tari_wallet_core", core_dir, dirs_exist_ok=True)
    
    print("  ✓ Created core package")
    
    print(f"\nPackage preparation complete!")
    print(f"Created packages in: {base_dir.absolute()}")
    print("\nNext steps:")
    print("1. Review generated package structures")
    print("2. Test packages locally: pip install -e ./dist_packages/tari_wallet_mainnet")
    print("3. Build packages: python -m build")
    print("4. Upload to PyPI: twine upload dist/*")


if __name__ == "__main__":
    main()
