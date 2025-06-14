//! Balance-related MCP tools for wallet operations
//!
//! This module provides comprehensive balance management tools including
//! balance queries, unspent amount tracking, and balance analysis.

use std::sync::Arc;

use minotari_app_grpc::tari_rpc::{Empty, GetBalanceRequest, UserPaymentId};
use minotari_mcp_common::{get_optional_string_param, security::PermissionLevel, McpError, McpResult, McpTool};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{json, Value};
use tonic::{transport::Channel, Request};

/// Tool for getting wallet balance
#[derive(Clone)]
pub struct GetBalanceTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetBalanceTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetBalanceTool {
    fn name(&self) -> &str {
        "get_balance"
    }

    fn description(&self) -> &str {
        "Retrieves wallet balance information including available, pending, and timelocked amounts"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "payment_id": {
                    "type": "string",
                    "description": "Optional payment ID to filter balance for specific transactions"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let payment_id = get_optional_string_param(&params, "payment_id").map(|payment_id_str| UserPaymentId {
            utf8_string: payment_id_str,
            ..Default::default()
        });

        let request = Request::new(GetBalanceRequest { payment_id });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .get_balance(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get balance: {}", e)))?
            .into_inner();

        // Calculate total balance and percentages
        let total_balance =
            response.available_balance + response.pending_incoming_balance + response.timelocked_balance;

        Ok(json!({
            "balance": {
                "available_balance": response.available_balance,
                "pending_incoming_balance": response.pending_incoming_balance,
                "pending_outgoing_balance": response.pending_outgoing_balance,
                "timelocked_balance": response.timelocked_balance,
            },
            "summary": {
                "total_balance": total_balance,
                "spendable_balance": response.available_balance,
                "locked_balance": response.pending_outgoing_balance + response.timelocked_balance,
                "incoming_balance": response.pending_incoming_balance,
            },
            "percentages": if total_balance > 0 {
                json!({
                    "available_percent": (response.available_balance as f64 / total_balance as f64 * 100.0).round(),
                    "pending_incoming_percent": (response.pending_incoming_balance as f64 / total_balance as f64 * 100.0).round(),
                    "timelocked_percent": (response.timelocked_balance as f64 / total_balance as f64 * 100.0).round(),
                })
            } else {
                json!({
                    "available_percent": 0.0,
                    "pending_incoming_percent": 0.0,
                    "timelocked_percent": 0.0,
                })
            },
            "status": {
                "has_funds": total_balance > 0,
                "can_spend": response.available_balance > 0,
                "has_pending": response.pending_incoming_balance > 0 || response.pending_outgoing_balance > 0,
                "has_timelocked": response.timelocked_balance > 0,
            }
        }))
    }
}

/// Tool for getting unspent amounts
#[derive(Clone)]
pub struct GetUnspentAmountsTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetUnspentAmountsTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetUnspentAmountsTool {
    fn name(&self) -> &str {
        "get_unspent_amounts"
    }

    fn description(&self) -> &str {
        "Retrieves the total value of all unspent transaction outputs in the wallet"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});

        let mut client = (*self.grpc_client).clone();
        let response = client
            .get_unspent_amounts(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get unspent amounts: {}", e)))?
            .into_inner();

        Ok(json!({
            "unspent_amount": response.amount,
            "formatted": {
                "tari": (response.amount.first().copied().unwrap_or(0) as f64 / 1_000_000.0).round() / 1.0, // Convert from µT to T
                "microtari": response.amount,
            },
            "info": {
                "description": "Total value of all unspent transaction outputs",
                "unit": "microTari (µT)",
                "note": "This represents UTXOs that can potentially be spent"
            }
        }))
    }
}

/// Tool for comprehensive balance analysis
#[derive(Clone)]
pub struct BalanceAnalysisTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl BalanceAnalysisTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for BalanceAnalysisTool {
    fn name(&self) -> &str {
        "balance_analysis"
    }

    fn description(&self) -> &str {
        "Provides comprehensive balance analysis including liquidity assessment and spending recommendations"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "requested_amount": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Optional amount to analyze spending feasibility for (in microTari)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        // Get balance information
        let balance_request = Request::new(GetBalanceRequest { payment_id: None });
        let mut client = (*self.grpc_client).clone();
        let balance_response = client
            .get_balance(balance_request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get balance: {}", e)))?
            .into_inner();

        // Get unspent amounts
        let unspent_request = Request::new(Empty {});
        let mut client2 = (*self.grpc_client).clone();
        let unspent_response = client2
            .get_unspent_amounts(unspent_request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get unspent amounts: {}", e)))?
            .into_inner();

        let requested_amount = params.get("requested_amount").and_then(|v| v.as_u64()).unwrap_or(0);

        // Calculate various metrics
        let total_balance = balance_response.available_balance +
            balance_response.pending_incoming_balance +
            balance_response.timelocked_balance;

        let liquid_balance = balance_response.available_balance;
        let locked_balance = balance_response.pending_outgoing_balance + balance_response.timelocked_balance;

        // Liquidity analysis
        let liquidity_ratio = if total_balance > 0 {
            liquid_balance as f64 / total_balance as f64
        } else {
            0.0
        };

        let liquidity_status = match liquidity_ratio {
            r if r >= 0.8 => "EXCELLENT",
            r if r >= 0.6 => "GOOD",
            r if r >= 0.4 => "MODERATE",
            r if r >= 0.2 => "LIMITED",
            _ => "POOR",
        };

        // Spending analysis
        let can_spend_requested = requested_amount > 0 && liquid_balance >= requested_amount;
        let spending_capacity = liquid_balance;

        // Generate recommendations
        let mut recommendations = Vec::new();

        if liquid_balance == 0 && total_balance > 0 {
            recommendations.push("All funds are locked or pending - wait for transactions to confirm".to_string());
        } else if liquidity_ratio < 0.5 && total_balance > 0 {
            recommendations.push("Consider reducing pending transactions to improve liquidity".to_string());
        }

        if requested_amount > 0 {
            if !can_spend_requested {
                if liquid_balance == 0 {
                    recommendations.push("No funds available for spending".to_string());
                } else {
                    recommendations.push(format!(
                        "Requested amount ({} µT) exceeds available balance ({} µT)",
                        requested_amount, liquid_balance
                    ));
                }
            } else {
                let remaining_after = liquid_balance - requested_amount;
                if remaining_after < 10000 {
                    // Less than 0.01 T remaining
                    recommendations.push("Transaction would use almost all available funds".to_string());
                }
            }
        }

        if balance_response.pending_incoming_balance > 0 {
            recommendations.push(format!(
                "Incoming funds ({} µT) will be available once confirmed",
                balance_response.pending_incoming_balance
            ));
        }

        Ok(json!({
            "balance_overview": {
                "total_balance": total_balance,
                "liquid_balance": liquid_balance,
                "locked_balance": locked_balance,
                "pending_incoming": balance_response.pending_incoming_balance,
                "pending_outgoing": balance_response.pending_outgoing_balance,
                "timelocked": balance_response.timelocked_balance,
                "unspent_utxos_value": unspent_response.amount,
            },
            "liquidity_analysis": {
                "liquidity_ratio": (liquidity_ratio * 100.0).round(),
                "liquidity_status": liquidity_status,
                "spending_capacity": spending_capacity,
                "description": format!(
                    "{}% of total balance is immediately spendable",
                    (liquidity_ratio * 100.0).round()
                ),
            },
            "spending_analysis": if requested_amount > 0 {
                json!({
                    "requested_amount": requested_amount,
                    "can_spend": can_spend_requested,
                    "remaining_after_spend": if can_spend_requested {
                        liquid_balance - requested_amount
                    } else {
                        liquid_balance
                    },
                    "deficit": if !can_spend_requested && requested_amount > liquid_balance {
                        requested_amount - liquid_balance
                    } else {
                        0
                    },
                })
            } else {
                json!(null)
            },
            "wallet_health": {
                "overall_status": if total_balance > 0 {
                    if liquid_balance > total_balance / 2 {
                        "HEALTHY"
                    } else {
                        "ATTENTION_NEEDED"
                    }
                } else {
                    "EMPTY"
                },
                "has_funds": total_balance > 0,
                "can_transact": liquid_balance > 0,
                "transaction_capacity": match liquid_balance {
                    0 => "NONE",
                    1..=99999 => "MICRO", // Less than 0.1 T
                    100000..=999999 => "SMALL", // 0.1 - 1 T
                    1000000..=9999999 => "MEDIUM", // 1 - 10 T
                    _ => "LARGE", // More than 10 T
                },
            },
            "recommendations": recommendations,
            "formatted_balances": {
                "total_tari": (total_balance as f64 / 1_000_000.0),
                "available_tari": (liquid_balance as f64 / 1_000_000.0),
                "locked_tari": (locked_balance as f64 / 1_000_000.0),
                "pending_incoming_tari": (balance_response.pending_incoming_balance as f64 / 1_000_000.0),
            },
            "analysis_timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }))
    }
}

/// Tool for balance monitoring and alerts
#[derive(Clone)]
pub struct BalanceMonitorTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl BalanceMonitorTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for BalanceMonitorTool {
    fn name(&self) -> &str {
        "balance_monitor"
    }

    fn description(&self) -> &str {
        "Monitors balance status and provides alerts based on configurable thresholds"
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "low_balance_threshold": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Threshold for low balance alerts (in microTari). Default: 1,000,000 µT (1 T)"
                },
                "high_pending_threshold": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Threshold for high pending balance alerts (in microTari). Default: 5,000,000 µT (5 T)"
                },
                "liquidity_threshold": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Liquidity ratio threshold (0-1). Default: 0.5 (50%)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        // Get configurable thresholds
        let low_balance_threshold = params
            .get("low_balance_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(1_000_000); // Default 1 T

        let high_pending_threshold = params
            .get("high_pending_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(5_000_000); // Default 5 T

        let liquidity_threshold = params
            .get("liquidity_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5); // Default 50%

        // Get current balance
        let balance_request = Request::new(GetBalanceRequest { payment_id: None });
        let mut client = (*self.grpc_client).clone();
        let balance_response = client
            .get_balance(balance_request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get balance: {}", e)))?
            .into_inner();

        let total_balance = balance_response.available_balance +
            balance_response.pending_incoming_balance +
            balance_response.timelocked_balance;

        let liquidity_ratio = if total_balance > 0 {
            balance_response.available_balance as f64 / total_balance as f64
        } else {
            0.0
        };

        // Generate alerts
        let mut alerts = Vec::new();
        let mut alert_level = "INFO";

        // Check balance thresholds
        if balance_response.available_balance < low_balance_threshold {
            alerts.push(json!({
                "type": "LOW_BALANCE",
                "severity": "WARNING",
                "message": format!(
                    "Available balance ({} µT) is below threshold ({} µT)",
                    balance_response.available_balance, low_balance_threshold
                ),
                "available_balance": balance_response.available_balance,
                "threshold": low_balance_threshold,
            }));
            alert_level = "WARNING";
        }

        // Check pending amounts
        if balance_response.pending_outgoing_balance > high_pending_threshold {
            alerts.push(json!({
                "type": "HIGH_PENDING_OUTGOING",
                "severity": "WARNING",
                "message": format!(
                    "High pending outgoing balance ({} µT) detected",
                    balance_response.pending_outgoing_balance
                ),
                "pending_amount": balance_response.pending_outgoing_balance,
                "threshold": high_pending_threshold,
            }));
            alert_level = "WARNING";
        }

        // Check liquidity
        if liquidity_ratio < liquidity_threshold && total_balance > 0 {
            alerts.push(json!({
                "type": "LOW_LIQUIDITY",
                "severity": "CAUTION",
                "message": format!(
                    "Low liquidity ratio ({:.1}%) - most funds are locked or pending",
                    liquidity_ratio * 100.0
                ),
                "liquidity_ratio": liquidity_ratio,
                "threshold": liquidity_threshold,
            }));
            if alert_level == "INFO" {
                alert_level = "CAUTION";
            }
        }

        // Check for zero balance
        if total_balance == 0 {
            alerts.push(json!({
                "type": "ZERO_BALANCE",
                "severity": "CRITICAL",
                "message": "Wallet has no funds",
                "total_balance": total_balance,
            }));
            alert_level = "CRITICAL";
        }

        // Check for stuck transactions (high pending for extended period)
        if balance_response.pending_outgoing_balance > 0 && balance_response.pending_incoming_balance == 0 {
            alerts.push(json!({
                "type": "POTENTIAL_STUCK_TRANSACTION",
                "severity": "INFO",
                "message": "Outgoing transactions pending - monitor for completion",
                "pending_outgoing": balance_response.pending_outgoing_balance,
            }));
        }

        if alerts.is_empty() {
            alerts.push(json!({
                "type": "BALANCE_HEALTHY",
                "severity": "INFO",
                "message": "All balance metrics are within normal ranges",
            }));
        }

        Ok(json!({
            "monitor_status": {
                "alert_level": alert_level,
                "total_alerts": alerts.len(),
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            },
            "alerts": alerts,
            "current_balance": {
                "available": balance_response.available_balance,
                "pending_incoming": balance_response.pending_incoming_balance,
                "pending_outgoing": balance_response.pending_outgoing_balance,
                "timelocked": balance_response.timelocked_balance,
                "total": total_balance,
                "liquidity_ratio": (liquidity_ratio * 100.0).round(),
            },
            "thresholds": {
                "low_balance_threshold": low_balance_threshold,
                "high_pending_threshold": high_pending_threshold,
                "liquidity_threshold": (liquidity_threshold * 100.0).round(),
            },
            "recommendations": if alert_level != "INFO" {
                match alert_level {
                    "CRITICAL" => vec!["Fund wallet immediately", "Check for incoming transactions"],
                    "WARNING" => vec!["Monitor balance closely", "Consider reducing transaction frequency", "Check transaction status"],
                    "CAUTION" => vec!["Review pending transactions", "Ensure adequate liquidity for future transactions"],
                    _ => vec![]
                }
            } else {
                vec!["Balance monitoring active - no immediate action required"]
            }
        }))
    }
}
