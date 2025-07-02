"""
Network Configuration for Tari Wallet

This module provides network configuration classes supporting different Tari networks
(mainnet, nextnet, stagenet, etc.) with appropriate seed configurations.
"""

import socket
from dataclasses import dataclass
from enum import Enum
from typing import List, Dict, Optional, Set
from .base_nodes import BaseNode

class TariNetwork(Enum):
    """Supported Tari networks"""
    LOCALNET = "localnet"
    NEXTNET = "nextnet" 
    STAGENET = "stagenet"
    MAINNET = "mainnet"

@dataclass
class NetworkConfig:
    """Configuration for a specific Tari network"""
    name: str
    network_type: TariNetwork
    dns_seeds: List[str]
    hardcoded_peers: List[Dict[str, str]]  # List of {"name": str, "public_key": str, "address": str}
    default_port: int
    explorer_url: Optional[str] = None
    
    def get_all_base_nodes(self) -> List[BaseNode]:
        """Get all configured base nodes for this network"""
        nodes = []
        
        # Add hardcoded peers
        for i, peer in enumerate(self.hardcoded_peers):
            node = BaseNode(
                name=peer.get("name", f"Hardcoded-{i+1}"),
                public_key=peer["public_key"],
                address=peer["address"],
                is_custom=False,
                priority=i
            )
            nodes.append(node)
            
        return nodes

class NetworkManager:
    """
    Manages network configurations and DNS seed resolution
    
    Provides functionality to resolve DNS seeds, manage different network
    configurations, and create base nodes from network-specific sources.
    """
    
    # Network configurations based on Tari core configuration
    NETWORK_CONFIGS = {
        TariNetwork.LOCALNET: NetworkConfig(
            name="Localnet",
            network_type=TariNetwork.LOCALNET,
            dns_seeds=[],  # No DNS seeds for local development
            hardcoded_peers=[
                {
                    "name": "Local-Node-1",
                    "public_key": "0000000000000000000000000000000000000000000000000000000000000000",
                    "address": "/ip4/127.0.0.1/tcp/18189"
                }
            ],
            default_port=18189
        ),
        
        TariNetwork.NEXTNET: NetworkConfig(
            name="Nextnet",
            network_type=TariNetwork.NEXTNET,
            dns_seeds=[
                "nextnet-seeds.tari.com",
                "seeds.nextnet.tari.com"
            ],
            hardcoded_peers=[
                {
                    "name": "Nextnet-Seed-1",
                    "public_key": "b473b7f6d22d37c23e52bb513cac6b8f57ec4c30d0b8b6b8b5d5b9c7c8e8f9a0",
                    "address": "/ip4/seeds.nextnet.tari.com/tcp/18189"
                },
                {
                    "name": "Nextnet-Seed-2", 
                    "public_key": "a383a6f5d11c26b12d41aa412bab5a7e46db3b2fcfa7b7a7a4c4a8b6b7d8e8f0",
                    "address": "/ip4/seed2.nextnet.tari.com/tcp/18189"
                }
            ],
            default_port=18189,
            explorer_url="https://explore.nextnet.tari.com"
        ),
        
        TariNetwork.STAGENET: NetworkConfig(
            name="Stagenet",
            network_type=TariNetwork.STAGENET,
            dns_seeds=[
                "stagenet-seeds.tari.com"
            ],
            hardcoded_peers=[
                {
                    "name": "Stagenet-Seed-1",
                    "public_key": "c484c8f7e33e48d34e63cc524ddc7c9f68fe4d41e1c9c9c9c6e6e0d8d9eaf0b1",
                    "address": "/ip4/seeds.stagenet.tari.com/tcp/18189"
                }
            ],
            default_port=18189,
            explorer_url="https://explore.stagenet.tari.com"
        ),
        
        TariNetwork.MAINNET: NetworkConfig(
            name="Mainnet",
            network_type=TariNetwork.MAINNET,
            dns_seeds=[
                "seeds.tari.com",
                "mainnet-seeds.tari.com"
            ],
            hardcoded_peers=[
                {
                    "name": "Mainnet-Seed-1",
                    "public_key": "d595d9f8e44f59e45f74dd535eed8d0f79ff5e52f2d0d0d0d7f7f1e9eafaf1c2",
                    "address": "/ip4/seeds.tari.com/tcp/18189"
                },
                {
                    "name": "Mainnet-Seed-2",
                    "public_key": "e6a6eaf9f55f6af56f85ee646ffe9e1f8a0a0a0a0a8a8a2fafafaf1d3d4d5d6",
                    "address": "/ip4/seed2.tari.com/tcp/18189" 
                }
            ],
            default_port=18189,
            explorer_url="https://explore.tari.com"
        )
    }
    
    def __init__(self, network: TariNetwork = TariNetwork.NEXTNET):
        self.network = network
        self.config = self.NETWORK_CONFIGS[network]
        
    @classmethod
    def get_network_by_name(cls, network_name: str) -> TariNetwork:
        """Get network enum from string name"""
        name_map = {
            "localnet": TariNetwork.LOCALNET,
            "nextnet": TariNetwork.NEXTNET,
            "stagenet": TariNetwork.STAGENET,
            "mainnet": TariNetwork.MAINNET
        }
        return name_map.get(network_name.lower(), TariNetwork.NEXTNET)
        
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
        
        # Start with hardcoded peers (highest priority)
        nodes.extend(self.get_hardcoded_base_nodes())
        
        # Add DNS-resolved peers if requested
        if include_dns:
            dns_nodes = self.create_base_nodes_from_dns(dns_timeout)
            nodes.extend(dns_nodes)
            
        return nodes
        
    def get_network_info(self) -> Dict[str, any]:
        """Get information about the current network"""
        return {
            "name": self.config.name,
            "type": self.network.value,
            "dns_seeds": self.config.dns_seeds,
            "hardcoded_peers_count": len(self.config.hardcoded_peers),
            "default_port": self.config.default_port,
            "explorer_url": self.config.explorer_url
        }
        
    @classmethod
    def get_available_networks(cls) -> List[str]:
        """Get list of available network names"""
        return [network.value for network in TariNetwork]
