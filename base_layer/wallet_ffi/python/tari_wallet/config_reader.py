"""
Configuration Reader for Tari Networks

This module reads the actual Tari network configuration from 
common/config/presets/b_peer_seeds.toml instead of hardcoding values.
"""

import os
import re
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from .base_nodes import BaseNode

class TariConfigReader:
    """
    Reads Tari network configuration from the actual config files
    
    This class parses the b_peer_seeds.toml file to get real DNS seeds
    and peer seeds for each network, ensuring consistency with the core
    Tari implementation.
    """
    
    def __init__(self, config_path: Optional[str] = None):
        """
        Initialize the config reader
        
        Args:
            config_path: Path to b_peer_seeds.toml (auto-detects if None)
        """
        self.config_path = config_path or self._find_config_file()
        self.config_data = None
        
    def _find_config_file(self) -> Optional[str]:
        """
        Try to find the b_peer_seeds.toml file in common locations
        
        Returns:
            Path to the config file or None if not found
        """
        # Common paths relative to the project root
        possible_paths = [
            "common/config/presets/b_peer_seeds.toml",
            "../../../common/config/presets/b_peer_seeds.toml",  # From wallet_ffi
            "../../../../common/config/presets/b_peer_seeds.toml",  # From python module
        ]
        
        # Try to find from current working directory or relative paths
        for rel_path in possible_paths:
            for base_dir in [os.getcwd(), Path(__file__).parent, Path(__file__).parent.parent.parent.parent.parent]:
                config_path = Path(base_dir) / rel_path
                if config_path.exists():
                    return str(config_path)
                    
        return None
        
    def _parse_toml_simple(self, content: str) -> Dict:
        """
        Simple TOML parser for the peer seeds config format
        
        This is a minimal parser that handles the specific format of 
        b_peer_seeds.toml without requiring external TOML libraries.
        Handles multi-line arrays correctly.
        
        Args:
            content: Raw TOML content
            
        Returns:
            Parsed configuration dictionary
        """
        config = {}
        current_section = None
        current_array_key = None
        current_array_items = []
        in_multiline_array = False
        
        lines = content.split('\n')
        
        for line in lines:
            original_line = line
            line = line.strip()
            
            # Skip comments and empty lines (but not if we're in a multiline array)
            if (not line or line.startswith('#')) and not in_multiline_array:
                continue
                
            # Section headers [network.p2p.seeds]
            if line.startswith('[') and line.endswith(']') and not in_multiline_array:
                section_name = line[1:-1]
                # Extract just the network name (e.g., "nextnet" from "nextnet.p2p.seeds")
                if '.p2p.seeds' in section_name:
                    network_name = section_name.replace('.p2p.seeds', '')
                    current_section = network_name
                    if current_section not in config:
                        config[current_section] = {}
                continue
                
            # Skip lines outside of relevant sections
            if current_section is None:
                continue
            
            # Handle multiline arrays
            if in_multiline_array:
                # Check for end of array
                if ']' in line:
                    # Extract any remaining items before the closing bracket
                    parts = line.split(']')[0].strip()
                    if parts and parts != ',':
                        # Clean up the item
                        item = parts.strip(' ,')
                        if item.startswith('"') and item.endswith('"'):
                            item = item[1:-1]
                        if item:
                            current_array_items.append(item)
                    
                    # Store the array and reset
                    config[current_section][current_array_key] = current_array_items
                    in_multiline_array = False
                    current_array_key = None
                    current_array_items = []
                else:
                    # Extract array item from this line
                    item = line.strip(' ,')
                    if item and not item.startswith('#'):
                        # Remove quotes
                        if item.startswith('"') and item.endswith('",'):
                            item = item[1:-2]
                        elif item.startswith('"') and item.endswith('"'):
                            item = item[1:-1]
                        if item:
                            current_array_items.append(item)
                continue
                
            # Parse key-value pairs
            if '=' in line:
                key, value = line.split('=', 1)
                key = key.strip()
                value = value.strip()
                
                # Handle arrays
                if value.startswith('['):
                    if value.endswith(']'):
                        # Single line array
                        array_content = value[1:-1].strip()
                        if array_content:
                            # Split by comma and clean up quotes
                            items = []
                            parts = array_content.split('",')
                            for part in parts:
                                item = part.strip(' "')
                                if item and not item.startswith('#'):
                                    items.append(item)
                            config[current_section][key] = items
                        else:
                            config[current_section][key] = []
                    else:
                        # Start of multiline array
                        in_multiline_array = True
                        current_array_key = key
                        current_array_items = []
                        
                        # Check if there's an item on the same line as the opening [
                        remaining = value[1:].strip()
                        if remaining and not remaining.startswith('#'):
                            item = remaining.strip(' ",')
                            if item.startswith('"') and item.endswith('"'):
                                item = item[1:-1]
                            if item:
                                current_array_items.append(item)
                else:
                    # Single value (remove quotes)
                    config[current_section][key] = value.strip('"')
                    
        return config
        
    def load_config(self) -> bool:
        """
        Load and parse the configuration file
        
        Returns:
            True if successfully loaded, False otherwise
        """
        if not self.config_path or not os.path.exists(self.config_path):
            return False
            
        try:
            with open(self.config_path, 'r', encoding='utf-8') as f:
                content = f.read()
                
            self.config_data = self._parse_toml_simple(content)
            return True
            
        except Exception as e:
            print(f"Error loading config from {self.config_path}: {e}")
            return False
            
    def get_network_config(self, network: str) -> Dict[str, List[str]]:
        """
        Get configuration for a specific network
        
        Args:
            network: Network name (nextnet, mainnet, etc.)
            
        Returns:
            Dictionary with 'dns_seeds' and 'peer_seeds' lists
        """
        if not self.config_data:
            if not self.load_config():
                return {"dns_seeds": [], "peer_seeds": []}
                
        network_config = self.config_data.get(network, {})
        
        return {
            "dns_seeds": network_config.get("dns_seeds", []),
            "peer_seeds": network_config.get("peer_seeds", [])
        }
        
    def parse_peer_seed(self, peer_seed: str) -> Optional[Tuple[str, str]]:
        """
        Parse a peer seed string into public key and address
        
        Format: "pubkey::/ip4/address/tcp/port" or similar
        
        Args:
            peer_seed: Peer seed string from config
            
        Returns:
            Tuple of (public_key, address) or None if invalid
        """
        if '::' not in peer_seed:
            return None
            
        try:
            public_key, address = peer_seed.split('::', 1)
            return public_key.strip(), address.strip()
        except Exception:
            return None
            
    def create_base_nodes_from_config(self, network: str) -> List[BaseNode]:
        """
        Create BaseNode objects from the configuration for a specific network
        
        Args:
            network: Network name
            
        Returns:
            List of BaseNode objects created from config
        """
        config = self.get_network_config(network)
        nodes = []
        
        # Create nodes from peer_seeds
        for i, peer_seed in enumerate(config.get("peer_seeds", [])):
            parsed = self.parse_peer_seed(peer_seed)
            if parsed:
                public_key, address = parsed
                node = BaseNode(
                    name=f"{network.title()}-Seed-{i+1}",
                    public_key=public_key,
                    address=address,
                    is_custom=False,
                    priority=i
                )
                nodes.append(node)
                
        return nodes
        
    def get_dns_seeds(self, network: str) -> List[str]:
        """
        Get DNS seeds for a specific network
        
        Args:
            network: Network name
            
        Returns:
            List of DNS seed hostnames
        """
        config = self.get_network_config(network)
        return config.get("dns_seeds", [])
        
    def get_available_networks(self) -> List[str]:
        """
        Get list of available networks from the configuration
        
        Returns:
            List of network names found in config
        """
        if not self.config_data:
            if not self.load_config():
                return []
                
        # Filter out the 'p2p' section which is not a network
        networks = [name for name in self.config_data.keys() if name != 'p2p']
        return sorted(networks)
        
    def get_network_info(self, network: str) -> Dict[str, any]:
        """
        Get comprehensive information about a network
        
        Args:
            network: Network name
            
        Returns:
            Dictionary with network information
        """
        config = self.get_network_config(network)
        nodes = self.create_base_nodes_from_config(network)
        
        return {
            "name": network,
            "dns_seeds": config.get("dns_seeds", []),
            "dns_seeds_count": len(config.get("dns_seeds", [])),
            "peer_seeds_count": len(config.get("peer_seeds", [])),
            "total_configured_nodes": len(nodes),
            "config_file_path": self.config_path,
            "supported_transports": self._analyze_transports(config.get("peer_seeds", []))
        }
        
    def _analyze_transports(self, peer_seeds: List[str]) -> Dict[str, int]:
        """
        Analyze the transport types used in peer seeds
        
        Args:
            peer_seeds: List of peer seed strings
            
        Returns:
            Dictionary with transport type counts
        """
        transports = {"ip4": 0, "ip6": 0, "onion3": 0, "other": 0}
        
        for seed in peer_seeds:
            if "::/ip4/" in seed:
                transports["ip4"] += 1
            elif "::/ip6/" in seed:
                transports["ip6"] += 1
            elif "::/onion3/" in seed:
                transports["onion3"] += 1
            else:
                transports["other"] += 1
                
        return transports

# Global instance for easy access
_config_reader = None

def get_config_reader() -> TariConfigReader:
    """
    Get a global instance of the config reader
    
    Returns:
        TariConfigReader instance
    """
    global _config_reader
    if _config_reader is None:
        _config_reader = TariConfigReader()
    return _config_reader

def read_network_config(network: str) -> Dict[str, List[str]]:
    """
    Convenience function to read network configuration
    
    Args:
        network: Network name
        
    Returns:
        Network configuration dictionary
    """
    reader = get_config_reader()
    return reader.get_network_config(network)

def create_nodes_from_config(network: str) -> List[BaseNode]:
    """
    Convenience function to create BaseNode objects from config
    
    Args:
        network: Network name
        
    Returns:
        List of BaseNode objects
    """
    reader = get_config_reader()
    return reader.create_base_nodes_from_config(network)
