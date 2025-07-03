"""
Wallet Sync Manager

This module provides explicit sync and refresh operations for base node connectivity,
implementing peer caching for instant subsequent connections.
"""

import time
import json
import os
from typing import List, Dict, Any, Optional, Tuple
from dataclasses import dataclass
from datetime import datetime, timedelta

from .base_nodes import BaseNode
from .network import TariNetwork
from .discovery import SimpleDiscoveryService


@dataclass
class PeerCache:
    """Cache information for successful peer connections"""
    public_key: str
    address: str
    last_successful_connection: float
    connection_count: int = 0
    average_connection_time: float = 0.0
    
    def update_connection_success(self, connection_time: float):
        """Update cache with successful connection"""
        self.last_successful_connection = time.time()
        self.connection_count += 1
        # Update running average
        if self.average_connection_time == 0.0:
            self.average_connection_time = connection_time
        else:
            self.average_connection_time = (
                (self.average_connection_time * (self.connection_count - 1) + connection_time) 
                / self.connection_count
            )


class WalletSyncManager:
    """
    Manages explicit sync operations and peer caching for instant subsequent connections
    
    Provides the sync infrastructure to achieve "instant subsequent transactions" by
    caching successful peer connections and reusing them efficiently.
    """
    
    def __init__(self, 
                 network: TariNetwork,
                 cache_file_path: Optional[str] = None,
                 cache_ttl_hours: int = 24):
        self.network = network
        self.cache_file_path = cache_file_path
        self.cache_ttl_hours = cache_ttl_hours
        self.peer_cache: Dict[str, PeerCache] = {}
        self.last_refresh_time: Optional[float] = None
        
        # Load existing cache
        if self.cache_file_path and os.path.exists(self.cache_file_path):
            self._load_peer_cache()
    
    def _load_peer_cache(self):
        """Load peer cache from file"""
        try:
            with open(self.cache_file_path, 'r') as f:
                data = json.load(f)
                
                # Load cached peers
                for public_key, cache_data in data.get('peers', {}).items():
                    self.peer_cache[public_key] = PeerCache(
                        public_key=public_key,
                        address=cache_data['address'],
                        last_successful_connection=cache_data['last_successful_connection'],
                        connection_count=cache_data.get('connection_count', 0),
                        average_connection_time=cache_data.get('average_connection_time', 0.0)
                    )
                
                self.last_refresh_time = data.get('last_refresh_time')
                
        except (FileNotFoundError, json.JSONDecodeError, KeyError) as e:
            # If cache file is corrupted, start fresh
            self.peer_cache = {}
            self.last_refresh_time = None
    
    def _save_peer_cache(self):
        """Save peer cache to file"""
        if not self.cache_file_path:
            return
            
        try:
            os.makedirs(os.path.dirname(self.cache_file_path), exist_ok=True)
            
            # Convert cache to serializable format
            cache_data = {}
            for public_key, cache in self.peer_cache.items():
                cache_data[public_key] = {
                    'address': cache.address,
                    'last_successful_connection': cache.last_successful_connection,
                    'connection_count': cache.connection_count,
                    'average_connection_time': cache.average_connection_time
                }
            
            data = {
                'peers': cache_data,
                'last_refresh_time': self.last_refresh_time,
                'cache_version': '1.0',
                'network': self.network.network_name,
                'last_updated': time.time()
            }
            
            with open(self.cache_file_path, 'w') as f:
                json.dump(data, f, indent=2)
                
        except Exception as e:
            print(f"Warning: Could not save peer cache: {e}")
    
    def get_cache_statistics(self) -> Dict[str, Any]:
        """Get statistics about the peer cache"""
        current_time = time.time()
        cache_ttl_seconds = self.cache_ttl_hours * 3600
        
        valid_entries = 0
        total_connections = 0
        avg_connection_times = []
        
        for cache in self.peer_cache.values():
            if (current_time - cache.last_successful_connection) < cache_ttl_seconds:
                valid_entries += 1
            total_connections += cache.connection_count
            if cache.average_connection_time > 0:
                avg_connection_times.append(cache.average_connection_time)
        
        return {
            "total_cached_peers": len(self.peer_cache),
            "valid_cached_peers": valid_entries,
            "expired_cached_peers": len(self.peer_cache) - valid_entries,
            "total_cached_connections": total_connections,
            "average_connection_time": sum(avg_connection_times) / len(avg_connection_times) if avg_connection_times else 0.0,
            "last_refresh_age_seconds": time.time() - self.last_refresh_time if self.last_refresh_time else None,
            "cache_ttl_hours": self.cache_ttl_hours,
            "network": self.network.network_name
        }


def create_sync_manager_for_wallet(
    network: TariNetwork,
    wallet_datastore_path: str,
    wallet_database_name: str
) -> WalletSyncManager:
    """
    Create a sync manager with cache persistence in wallet directory
    
    Args:
        network: Target network
        wallet_datastore_path: Path to wallet data directory  
        wallet_database_name: Name of wallet database
        
    Returns:
        Configured WalletSyncManager instance
    """
    cache_file = os.path.join(
        wallet_datastore_path,
        f"{wallet_database_name}_peer_cache.json"
    )
    
    return WalletSyncManager(network, cache_file)
