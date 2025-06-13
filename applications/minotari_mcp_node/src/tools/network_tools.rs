//! Network-related MCP tools for base node operations
//!
//! This module provides tools for querying network status, peer information,
//! connectivity, and node identity.

use minotari_mcp_common::{McpTool, McpError, McpResult};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};

use tonic::transport::Channel;
use tonic::Request;
use minotari_app_grpc::tari_rpc::{
    Empty, GetPeersRequest,
};

/// Tool for getting network status
#[derive(Clone)]
pub struct GetNetworkStatusTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNetworkStatusTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNetworkStatusTool {
    fn name(&self) -> &str {
        "get_network_status"
    }
    
    fn description(&self) -> &str {
        "Retrieves base node network connectivity status and connection information"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.clone().get_network_status(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get network status: {}", e)))?
            .into_inner();
        
        let status = if response.num_node_connections >= 8 {
            "EXCELLENT"
        } else if response.num_node_connections >= 4 {
            "GOOD" 
        } else if response.num_node_connections >= 1 {
            "LIMITED"
        } else {
            "DISCONNECTED"
        };
        
        Ok(json!({
            "status": status,
            "avg_latency_ms": response.avg_latency_ms,
            "num_node_connections": response.num_node_connections,
            "connection_status": {
                "excellent": response.num_node_connections >= 8,
                "good": response.num_node_connections >= 4,
                "limited": response.num_node_connections >= 1,
                "disconnected": response.num_node_connections == 0,
            },
            "performance": {
                "latency_status": if response.avg_latency_ms <= 100 {
                    "EXCELLENT"
                } else if response.avg_latency_ms <= 300 {
                    "GOOD"
                } else if response.avg_latency_ms <= 1000 {
                    "FAIR"
                } else {
                    "POOR"
                },
                "recommended_min_connections": 4,
                "optimal_connections": 8,
            }
        }))
    }
}

/// Tool for listing connected peers
#[derive(Clone)]
pub struct ListConnectedPeersTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl ListConnectedPeersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for ListConnectedPeersTool {
    fn name(&self) -> &str {
        "list_connected_peers"
    }
    
    fn description(&self) -> &str {
        "Lists all peers currently connected to the base node with detailed connection information"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.clone().list_connected_peers(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to list connected peers: {}", e)))?
            .into_inner();
        
        let connected_peers: Vec<Value> = response.connected_peers.iter().map(|peer| {
            json!({
                "public_key": hex::encode(&peer.public_key),
                "node_id": hex::encode(&peer.node_id),
                "addresses": peer.addresses.iter().map(|addr| {
                    format!("{}", String::from_utf8_lossy(&addr.address))
                }).collect::<Vec<_>>(),
                "last_connection": peer.last_connection,
                "flags": peer.flags,
                "banned_until": peer.banned_until,
                "banned_reason": peer.banned_reason.clone(),
                "offline_at": peer.offline_at,
                "features": peer.features,
                "supported_protocols": peer.supported_protocols.iter()
                    .map(|proto| String::from_utf8_lossy(proto).to_string())
                    .collect::<Vec<_>>(),
                "user_agent": peer.user_agent.clone(),
                "connection_status": {
                    "is_banned": peer.banned_until > 0,
                    "is_online": peer.offline_at == 0,
                    "connection_age_seconds": if peer.last_connection > 0 {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() - peer.last_connection
                    } else {
                        0
                    }
                }
            })
        }).collect();
        
        let peer_count = connected_peers.len();
        let banned_count = connected_peers.iter()
            .filter(|peer| peer["connection_status"]["is_banned"].as_bool().unwrap_or(false))
            .count();
        let online_count = connected_peers.iter()
            .filter(|peer| peer["connection_status"]["is_online"].as_bool().unwrap_or(false))
            .count();
        
        Ok(json!({
            "connected_peers": connected_peers,
            "summary": {
                "total_peers": peer_count,
                "online_peers": online_count,
                "banned_peers": banned_count,
                "healthy_peers": online_count - banned_count,
            },
            "network_health": {
                "status": if online_count >= 8 {
                    "EXCELLENT"
                } else if online_count >= 4 {
                    "GOOD"
                } else if online_count >= 1 {
                    "LIMITED"
                } else {
                    "DISCONNECTED"
                },
                "diversity_score": if peer_count > 0 {
                    // Simple diversity score based on unique user agents
                    let unique_user_agents: std::collections::HashSet<String> = connected_peers.iter()
                        .filter_map(|peer| peer["user_agent"].as_str())
                        .map(|ua| ua.to_string())
                        .collect();
                    (unique_user_agents.len() as f64 / peer_count as f64 * 100.0).round()
                } else {
                    0.0
                }
            }
        }))
    }
}

/// Tool for getting all peers (including disconnected)
#[derive(Clone)]
pub struct GetAllPeersTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetAllPeersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetAllPeersTool {
    fn name(&self) -> &str {
        "get_all_peers"
    }
    
    fn description(&self) -> &str {
        "Retrieves information about all known peers, including both connected and disconnected peers"
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let limit = params.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50); // Default limit to prevent overwhelming responses
        
        let request = Request::new(GetPeersRequest {});
        
        let mut response_stream = self.grpc_client.clone().get_peers(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get peers: {}", e)))?
            .into_inner();
        
        let mut all_peers = Vec::new();
        let mut count = 0;
        
        while let Some(peer_response) = response_stream.message().await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read peer stream: {}", e)))? {
            
            if count >= limit {
                break;
            }
            
            if let Some(peer) = peer_response.peer {
                let peer_info = json!({
                    "public_key": hex::encode(&peer.public_key),
                    "node_id": hex::encode(&peer.node_id),
                    "addresses": peer.addresses.iter().map(|addr| {
                        format!("{}", String::from_utf8_lossy(&addr.address))
                    }).collect::<Vec<_>>(),
                    "last_connection": peer.last_connection,
                    "flags": peer.flags,
                    "banned_until": peer.banned_until,
                    "banned_reason": peer.banned_reason,
                    "offline_at": peer.offline_at,
                    "features": peer.features,
                    "supported_protocols": peer.supported_protocols.iter()
                        .map(|proto| String::from_utf8_lossy(proto).to_string())
                        .collect::<Vec<_>>(),
                    "user_agent": peer.user_agent,
                    "status": {
                        "is_connected": peer.offline_at == 0,
                        "is_banned": peer.banned_until > 0,
                        "last_seen": if peer.last_connection > 0 {
                            peer.last_connection
                        } else {
                            peer.offline_at
                        },
                        "connection_attempts": peer.flags, // Assuming flags indicate connection attempts
                    }
                });
                
                all_peers.push(peer_info);
                count += 1;
            }
        }
        
        // Analyze peer data
        let connected_count = all_peers.iter()
            .filter(|peer| peer["status"]["is_connected"].as_bool().unwrap_or(false))
            .count();
        let banned_count = all_peers.iter()
            .filter(|peer| peer["status"]["is_banned"].as_bool().unwrap_or(false))
            .count();
        let total_count = all_peers.len();
        
        Ok(json!({
            "peers": all_peers,
            "summary": {
                "total_known_peers": total_count,
                "connected_peers": connected_count,
                "disconnected_peers": total_count - connected_count,
                "banned_peers": banned_count,
                "healthy_peers": connected_count - banned_count,
                "connection_rate": if total_count > 0 {
                    (connected_count as f64 / total_count as f64 * 100.0).round()
                } else {
                    0.0
                }
            },
            "metadata": {
                "limit_applied": limit,
                "results_truncated": count >= limit,
                "note": if count >= limit {
                    format!("Results limited to {} peers. Use limit parameter to adjust.", limit)
                } else {
                    "All known peers returned".to_string()
                }
            }
        }))
    }
}

/// Tool for getting node identity
#[derive(Clone)]
pub struct GetNodeIdentityTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNodeIdentityTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNodeIdentityTool {
    fn name(&self) -> &str {
        "get_node_identity"
    }
    
    fn description(&self) -> &str {
        "Retrieves the base node's network identity including public key and node ID"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.clone().identify(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get node identity: {}", e)))?
            .into_inner();
        
        Ok(json!({
            "public_key": hex::encode(&response.public_key),
            "node_id": hex::encode(&response.node_id),
            "public_address": response.public_address,
            "identity_info": {
                "public_key_length": response.public_key.len(),
                "node_id_length": response.node_id.len(),
                "has_public_address": !response.public_address.is_empty(),
            }
        }))
    }
}

/// Tool for network diagnostics
#[derive(Clone)]
pub struct NetworkDiagnosticsTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl NetworkDiagnosticsTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for NetworkDiagnosticsTool {
    fn name(&self) -> &str {
        "network_diagnostics"
    }
    
    fn description(&self) -> &str {
        "Performs comprehensive network diagnostics including connectivity, performance, and peer analysis"
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        // Get network status
        let status_request = Request::new(Empty {});
        let network_status = self.grpc_client.clone().get_network_status(status_request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get network status: {}", e)))?
            .into_inner();
        
        // Get connected peers
        let peers_request = Request::new(Empty {});
        let connected_peers = self.grpc_client.clone().list_connected_peers(peers_request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get connected peers: {}", e)))?
            .into_inner();
        
        // Get node identity
        let identity_request = Request::new(Empty {});
        let node_identity = self.grpc_client.clone().identify(identity_request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get node identity: {}", e)))?
            .into_inner();
        
        // Analyze network health
        let peer_count = connected_peers.connected_peers.len();
        let banned_peers = connected_peers.connected_peers.iter()
            .filter(|p| p.banned_until > 0)
            .count();
        
        let health_score = if peer_count >= 8 && network_status.avg_latency_ms <= 200 {
            100
        } else if peer_count >= 4 && network_status.avg_latency_ms <= 500 {
            75
        } else if peer_count >= 1 && network_status.avg_latency_ms <= 1000 {
            50
        } else if peer_count >= 1 {
            25
        } else {
            0
        };
        
        // Generate recommendations
        let mut recommendations = Vec::new();
        
        if peer_count < 4 {
            recommendations.push("Consider checking firewall settings or network connectivity - fewer than 4 peers connected".to_string());
        }
        if network_status.avg_latency_ms > 1000 {
            recommendations.push("High network latency detected - check internet connection quality".to_string());
        }
        if banned_peers > 0 {
            recommendations.push(format!("{} banned peers detected - this may indicate network issues", banned_peers));
        }
        if peer_count == 0 {
            recommendations.push("No peers connected - check network configuration and ensure ports are open".to_string());
        }
        
        if recommendations.is_empty() {
            recommendations.push("Network appears healthy - no issues detected".to_string());
        }
        
        Ok(json!({
            "network_health": {
                "overall_score": health_score,
                "status": match health_score {
                    90..=100 => "EXCELLENT",
                    70..=89 => "GOOD",
                    40..=69 => "FAIR",
                    1..=39 => "POOR",
                    _ => "CRITICAL",
                },
            },
            "connectivity": {
                "connected_peers": peer_count,
                "banned_peers": banned_peers,
                "healthy_peers": peer_count - banned_peers,
                "avg_latency_ms": network_status.avg_latency_ms,
                "num_node_connections": network_status.num_node_connections,
            },
            "node_identity": {
                "public_key": hex::encode(&node_identity.public_key),
                "node_id": hex::encode(&node_identity.node_id),
                "public_address": node_identity.public_address,
            },
            "peer_diversity": {
                "unique_user_agents": connected_peers.connected_peers.iter()
                    .map(|p| p.user_agent.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                "protocol_versions": connected_peers.connected_peers.iter()
                    .flat_map(|p| p.supported_protocols.iter())
                    .map(|proto| String::from_utf8_lossy(proto).to_string())
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
            },
            "recommendations": recommendations,
            "diagnostic_timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }))
    }
}
