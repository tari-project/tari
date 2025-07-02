"""
Network Configuration for Tari Wallet

This module provides network configuration classes reading from the actual 
Tari configuration files instead of hardcoded values.
"""

import socket
from dataclasses import dataclass
from enum import Enum
from typing import List, Dict, Optional, Set
from .base_nodes import BaseNode
from .config_reader import TariConfigReader, get_config_reader

class TariNetwork(Enum):
    """Supported Tari networks (dynamically loaded from config)"""
    LOCALNET = "localnet"
    NEXTNET = "nextnet" 
    STAGENET = "stagenet"
    MAINNET = "mainnet"
    ESMERALDA = "esmeralda"
    IGOR = "igor"

@dataclass
class NetworkConfig:
    """Configuration for a specific Tari network"""
    name: str
    network_type: TariNetwork
    dns_seeds: List[str]
    peer_seeds: List[BaseNode]
    default_port: int
    explorer_url: Optional[str] = None
    
    def get_all_base_nodes(self) -> List[BaseNode]:
        """Get all configured base nodes for this network"""
        return self.peer_seeds.copy()

class NetworkManager:
    """
    Manages network configurations and DNS seed resolution
    
    Reads configurations from the actual Tari config files and provides
    functionality to resolve DNS seeds and create base nodes.
    """
    
    # Default port for Tari networks
    DEFAULT_PORT = 18189
    
    # Explorer URLs for known networks
    EXPLORER_URLS = {
        "nextnet": "https://explore.nextnet.tari.com",
        "stagenet": "https://explore.stagenet.tari.com", 
        "mainnet": "https://explore.tari.com",
        "esmeralda": "https://explore.esmeralda.tari.com",
    }
    
    def __init__(self, network: TariNetwork = TariNetwork.NEXTNET):
        self.network = network
        self.config_reader = get_config_reader()
        self._network_config = None
        
    def _get_network_config(self) -> NetworkConfig:
        """Get the configuration for the current network"""
        if self._network_config is not None:
            return self._network_config
            
        network_name = self.network.value
        
        # Handle localnet specially since it's not in the config file
        if network_name == "localnet":
            self._network_config = NetworkConfig(
                name="Localnet",
                network_type=TariNetwork.LOCALNET,
                dns_seeds=[],
                peer_seeds=[
                    BaseNode(
                        name="Local-Node-1",
                        public_key="0000000000000000000000000000000000000000000000000000000000000000",
                        address="/ip4/127.0.0.1/tcp/18189",
                        is_custom=False,
                        priority=0
                    )
                ],
                default_port=self.DEFAULT_PORT
            )
        else:
            # Load from actual config file
            config_data = self.config_reader.get_network_config(network_name)
            peer_nodes = self.config_reader.create_base_nodes_from_config(network_name)
            
            self._network_config = NetworkConfig(
                name=network_name.title(),
                network_type=self.network,
                dns_seeds=config_data.get("dns_seeds", []),
                peer_seeds=peer_nodes,
                default_port=self.DEFAULT_PORT,
                explorer_url=self.EXPLORER_URLS.get(network_name)
            )
            
        return self._network_config
        
    @property
    def config(self) -> NetworkConfig:
        """Get the current network configuration"""
        return self._get_network_config()
        
    @classmethod
    def get_network_by_name(cls, network_name: str) -> TariNetwork:
        """Get network enum from string name"""
        try:
            return TariNetwork(network_name.lower())
        except ValueError:
            # Return nextnet as default for unknown networks
            return TariNetwork.NEXTNET
        
    def get_hardcoded_base_nodes(self) -> List[BaseNode]:
        """Get hardcoded base nodes for the current network"""
        return self.config.get_all_base_nodes()
        
    def resolve_dns_seeds(self, timeout: float = 5.0) -> List[str]:
        """
        Resolve DNS seeds to get peer addresses
        
        Args:
            timeout: DNS resolution timeout in seconds
            
        Returns:
            List of resolved IP addresses
        """
        resolved_ips = []
        
        for dns_seed in self.config.dns_seeds:
            try:
                # Set socket timeout for DNS resolution
                socket.setdefaulttimeout(timeout)
                
                # Resolve the DNS name to IP addresses
                addr_info = socket.getaddrinfo(dns_seed, self.config.default_port, socket.AF_UNSPEC)
                
                # Extract unique IP addresses
                ips = set()
                for family, type_, proto, canonname, sockaddr in addr_info:
                    if family in (socket.AF_INET, socket.AF_INET6):
                        ip = sockaddr[0]
                        ips.add(ip)
                        
                resolved_ips.extend(list(ips))
                
            except (socket.gaierror, socket.timeout, OSError) as e:
                # DNS resolution failed for this seed, continue with others
                print(f"Warning: Failed to resolve DNS seed {dns_seed}: {e}")
                continue
            finally:
                # Reset socket timeout
                socket.setdefaulttimeout(None)
                
        return list(set(resolved_ips))  # Remove duplicates
        
    def create_base_nodes_from_dns(self, timeout: float = 5.0) -> List[BaseNode]:
        """
        Create base nodes from DNS seed resolution
        
        Note: This creates placeholder nodes since we don't get public keys from DNS.
        In a real implementation, these would be discovered through the Tari protocol.
        
        Args:
            timeout: DNS resolution timeout in seconds
            
        Returns:
            List of BaseNode objects with placeholder public keys
        """
        resolved_ips = self.resolve_dns_seeds(timeout)
        nodes = []
        
        for i, ip in enumerate(resolved_ips):
            # Create placeholder node - in real implementation, public keys would come from peer discovery
            node = BaseNode(
                name=f"DNS-Seed-{i+1}",
                public_key=f"dns_placeholder_{i:032x}",  # Placeholder - would be discovered
                address=f"/ip4/{ip}/tcp/{self.config.default_port}",
                is_custom=False,
                priority=100 + i  # Lower priority than hardcoded peers
            )
            nodes.append(node)
            
        return nodes
        
    def get_all_discovered_nodes(self, include_dns: bool = True, dns_timeout: float = 5.0) -> List[BaseNode]:
        """
        Get all discovered base nodes for the current network
        
        Args:
            include_dns: Whether to include DNS-resolved nodes
            dns_timeout: Timeout for DNS resolution
            
        Returns:
            List of all discovered BaseNode objects
        """
        nodes = []
        
        # Start with configured peers (highest priority)
        nodes.extend(self.get_hardcoded_base_nodes())
        
        # Add DNS-resolved peers if requested
        if include_dns:
            dns_nodes = self.create_base_nodes_from_dns(dns_timeout)
            nodes.extend(dns_nodes)
            
        return nodes
        
    def get_network_info(self) -> Dict[str, any]:
        """Get information about the current network"""
        config = self.config
        
        # Get transport analysis from config reader
        config_reader_info = self.config_reader.get_network_info(self.network.value)
        
        return {
            "name": config.name,
            "type": self.network.value,
            "dns_seeds": config.dns_seeds,
            "dns_seeds_count": len(config.dns_seeds),
            "peer_seeds_count": len(config.peer_seeds),
            "default_port": config.default_port,
            "explorer_url": config.explorer_url,
            "config_file_loaded": self.config_reader.config_data is not None,
            "config_file_path": self.config_reader.config_path,
            "supported_transports": config_reader_info.get("supported_transports", {})
        }
        
    @classmethod
    def get_available_networks(cls) -> List[str]:
        """Get list of available network names"""
        # Get networks from config file plus localnet
        config_reader = get_config_reader()
        config_networks = config_reader.get_available_networks()
        
        # Always include localnet
        all_networks = ["localnet"] + config_networks
        return sorted(list(set(all_networks)))
        
    @classmethod 
    def get_all_network_info(cls) -> Dict[str, Dict]:
        """Get information about all available networks"""
        networks_info = {}
        
        for network_name in cls.get_available_networks():
            try:
                network = cls.get_network_by_name(network_name)
                manager = cls(network)
                networks_info[network_name] = manager.get_network_info()
            except Exception as e:
                networks_info[network_name] = {
                    "error": str(e),
                    "name": network_name,
                    "available": False
                }
                
        return networks_info
