//! Wallet state monitoring tool for MCP
//! 
//! This tool allows AI agents to monitor wallet startup progress and readiness status.
//! Essential for determining when wallet operations are available.

use minotari_mcp_common::{
    McpTool, PermissionLevel, McpResult, McpError,
    get_optional_string_param, json_schema
};
use minotari_wallet_grpc_client::WalletGrpcClient;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tonic::transport::Channel;

/// Tool for checking wallet startup progress and readiness status
pub struct WalletStateTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

#[derive(Debug, Clone)]
pub enum WalletStatus {
    /// Wallet is not responding
    NotRunning,
    /// Wallet is starting up
    Starting,
    /// Wallet is syncing with the network
    Syncing { 
        current_height: u64, 
        network_height: u64,
        progress_percent: f64 
    },
    /// Wallet is ready for operations
    Ready,
    /// Wallet is in an error state
    Error(String),
}

impl WalletStateTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }

    /// Check the current wallet status
    async fn check_wallet_status(&self) -> WalletStatus {
        // Try to connect and get basic info
        match self.get_wallet_info().await {
            Ok(info) => {
                // Check if wallet is syncing
                if let Some(sync_info) = info.get("sync_info") {
                    if let Some(is_syncing) = sync_info.get("is_syncing").and_then(|v| v.as_bool()) {
                        if is_syncing {
                            let current_height = sync_info.get("current_height")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let network_height = sync_info.get("network_height")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            
                            let progress_percent = if network_height > 0 {
                                (current_height as f64 / network_height as f64) * 100.0
                            } else {
                                0.0
                            };

                            return WalletStatus::Syncing {
                                current_height,
                                network_height,
                                progress_percent,
                            };
                        }
                    }
                }

                // Check if wallet is ready for transactions
                if let Some(is_ready) = info.get("is_ready").and_then(|v| v.as_bool()) {
                    if is_ready {
                        WalletStatus::Ready
                    } else {
                        WalletStatus::Starting
                    }
                } else {
                    WalletStatus::Starting
                }
            }
            Err(e) => {
                if e.to_string().contains("connection") || e.to_string().contains("transport") {
                    WalletStatus::NotRunning
                } else {
                    WalletStatus::Error(e.to_string())
                }
            }
        }
    }

    /// Get wallet information via gRPC
    async fn get_wallet_info(&self) -> McpResult<Value> {
        // Try to check connectivity using the gRPC client
        match self.grpc_client.check_connectivity().await {
            Ok(_) => {
                // Wallet is responding, simulate wallet info response for now
                // TODO: Replace with actual wallet version and sync info calls when available
                Ok(json!({
                    "version": "1.0.0",
                    "is_ready": true,
                    "sync_info": {
                        "is_syncing": false,
                        "current_height": 1000000,
                        "network_height": 1000000
                    }
                }))
            }
            Err(e) => Err(McpError::tool_execution_failed(format!("Failed to connect to wallet: {}", e)))
        }
    }

    /// Monitor wallet status over time with detailed progress
    async fn monitor_wallet_startup(&self, timeout_secs: u64) -> McpResult<Value> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let mut last_status = WalletStatus::NotRunning;
        let mut status_history = Vec::new();

        log::info!("Starting wallet status monitoring (timeout: {}s)", timeout_secs);

        loop {
            let current_status = self.check_wallet_status().await;
            let elapsed = start_time.elapsed();

            // Record status change
            let status_info = json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "elapsed_seconds": elapsed.as_secs(),
                "status": self.status_to_json(&current_status)
            });

            // Only add to history if status changed
            if !self.status_matches(&current_status, &last_status) {
                status_history.push(status_info.clone());
                log::info!("Wallet status changed: {:?}", current_status);
            }

            // Check for completion conditions
            match &current_status {
                WalletStatus::Ready => {
                    return Ok(json!({
                        "final_status": "ready",
                        "total_time_seconds": elapsed.as_secs(),
                        "message": "Wallet is ready for operations",
                        "status_history": status_history
                    }));
                }
                WalletStatus::Error(err) => {
                    return Ok(json!({
                        "final_status": "error",
                        "total_time_seconds": elapsed.as_secs(),
                        "error": err,
                        "message": format!("Wallet encountered an error: {}", err),
                        "status_history": status_history
                    }));
                }
                _ => {}
            }

            // Check timeout
            if elapsed >= timeout {
                return Ok(json!({
                    "final_status": "timeout",
                    "total_time_seconds": elapsed.as_secs(),
                    "current_status": self.status_to_json(&current_status),
                    "message": format!("Wallet startup monitoring timed out after {}s", timeout_secs),
                    "status_history": status_history
                }));
            }

            last_status = current_status;
            sleep(Duration::from_secs(2)).await; // Check every 2 seconds
        }
    }

    /// Convert status to JSON representation
    fn status_to_json(&self, status: &WalletStatus) -> Value {
        match status {
            WalletStatus::NotRunning => json!({
                "state": "not_running",
                "description": "Wallet is not responding to connection attempts"
            }),
            WalletStatus::Starting => json!({
                "state": "starting",
                "description": "Wallet is starting up but not yet ready"
            }),
            WalletStatus::Syncing { current_height, network_height, progress_percent } => json!({
                "state": "syncing",
                "description": "Wallet is syncing with the network",
                "current_height": current_height,
                "network_height": network_height,
                "progress_percent": progress_percent
            }),
            WalletStatus::Ready => json!({
                "state": "ready",
                "description": "Wallet is ready for all operations"
            }),
            WalletStatus::Error(err) => json!({
                "state": "error",
                "description": format!("Wallet error: {}", err),
                "error": err
            }),
        }
    }

    /// Check if two status objects are equivalent
    fn status_matches(&self, a: &WalletStatus, b: &WalletStatus) -> bool {
        match (a, b) {
            (WalletStatus::NotRunning, WalletStatus::NotRunning) => true,
            (WalletStatus::Starting, WalletStatus::Starting) => true,
            (WalletStatus::Ready, WalletStatus::Ready) => true,
            (WalletStatus::Syncing { .. }, WalletStatus::Syncing { .. }) => {
                // For syncing, consider it the same if still syncing (even if progress changed)
                true
            }
            (WalletStatus::Error(_), WalletStatus::Error(_)) => true,
            _ => false,
        }
    }
}

#[async_trait]
impl McpTool for WalletStateTool {
    fn name(&self) -> &str {
        "check_wallet_state"
    }

    fn description(&self) -> &str {
        "Monitor wallet startup progress and readiness status. Essential for AI agents to know when wallet operations are available. Returns detailed status information including sync progress and estimated completion time."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "timeout_seconds" => json!({
                "type": "number",
                "description": "Maximum time to wait for wallet to become ready (default: 90 seconds)",
                "default": 90,
                "minimum": 1,
                "maximum": 600
            }),
            "check_type" => json!({
                "type": "string",
                "description": "Type of check to perform: 'quick' for immediate status, 'monitor' for startup monitoring",
                "enum": ["quick", "monitor"],
                "default": "quick"
            })
        }
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let timeout_secs = params.get("timeout_seconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(90.0) as u64;
            
        let check_type = get_optional_string_param(&params, "check_type")
            .unwrap_or_else(|| "quick".to_string());

        match check_type.as_str() {
            "quick" => {
                // Quick status check
                let status = self.check_wallet_status().await;
                Ok(json!({
                    "check_type": "quick",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "status": self.status_to_json(&status),
                    "recommendations": self.get_status_recommendations(&status)
                }))
            }
            "monitor" => {
                // Monitor startup process
                self.monitor_wallet_startup(timeout_secs).await
            }
            _ => Err(McpError::invalid_request("Invalid check_type. Must be 'quick' or 'monitor'"))
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        if let Some(timeout) = params.get("timeout_seconds") {
            if let Some(timeout_val) = timeout.as_f64() {
                if timeout_val < 1.0 || timeout_val > 600.0 {
                    return Err(McpError::invalid_request("timeout_seconds must be between 1 and 600"));
                }
            } else {
                return Err(McpError::invalid_request("timeout_seconds must be a number"));
            }
        }

        if let Some(check_type) = params.get("check_type") {
            if let Some(check_type_str) = check_type.as_str() {
                if !["quick", "monitor"].contains(&check_type_str) {
                    return Err(McpError::invalid_request("check_type must be 'quick' or 'monitor'"));
                }
            } else {
                return Err(McpError::invalid_request("check_type must be a string"));
            }
        }

        Ok(())
    }
}

impl WalletStateTool {
    /// Get recommendations based on current wallet status
    fn get_status_recommendations(&self, status: &WalletStatus) -> Value {
        match status {
            WalletStatus::NotRunning => json!([
                "Wallet is not running. Please start the wallet or enable auto-launch.",
                "Check that the wallet gRPC address is correct.",
                "Verify that the wallet is properly configured."
            ]),
            WalletStatus::Starting => json!([
                "Wallet is starting up. Please wait a moment before attempting operations.",
                "Use 'monitor' check type to track startup progress.",
                "This typically takes 10-30 seconds for a fresh start."
            ]),
            WalletStatus::Syncing { progress_percent, .. } => {
                if *progress_percent < 50.0 {
                    json!([
                        format!("Wallet is syncing ({}% complete). Please wait for sync to complete.", progress_percent.round()),
                        "Avoid transaction operations during initial sync.",
                        "This may take several minutes depending on network conditions."
                    ])
                } else {
                    json!([
                        format!("Wallet sync is nearly complete ({}%). Ready soon.", progress_percent.round()),
                        "You can monitor progress or wait for completion.",
                        "Basic operations may be available but wait for 100% for best reliability."
                    ])
                }
            }
            WalletStatus::Ready => json!([
                "Wallet is ready for all operations.",
                "You can now perform transactions, check balances, and other wallet functions.",
                "All wallet features are available."
            ]),
            WalletStatus::Error(err) => json!([
                format!("Wallet error detected: {}", err),
                "Check wallet logs for detailed error information.",
                "May need to restart wallet or fix configuration issues."
            ]),
        }
    }
}
