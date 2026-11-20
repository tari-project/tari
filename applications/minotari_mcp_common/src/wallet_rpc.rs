// Copyright 2025, The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Wallet gRPC calls shared by the manual wallet MCP tools and the auto-discovered tools.

use minotari_wallet_grpc_client::{
    WalletGrpcClient,
    grpc::{
        CancelTransactionRequest,
        CoinSplitRequest,
        Empty,
        GetBalanceRequest,
        GetTransactionInfoRequest,
        PaymentRecipient,
        TransferRequest,
        UserPaymentId,
    },
};
use serde_json::{Value, json};
use tonic::transport::Channel;

use crate::{McpError, McpResult, get_optional_string_param, get_required_u64_param};

type Client = WalletGrpcClient<Channel>;

pub async fn get_balance(mut client: Client, params: &Value) -> McpResult<Value> {
    let payment_id = get_optional_string_param(params, "payment_id").map(|payment_id_str| UserPaymentId {
        utf8_string: payment_id_str,
        ..Default::default()
    });

    let response = client.get_balance(GetBalanceRequest { payment_id }).await?.into_inner();

    let total_balance = response
        .available_balance
        .saturating_add(response.pending_incoming_balance)
        .saturating_add(response.timelocked_balance);

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
            "locked_balance": response.pending_outgoing_balance.saturating_add(response.timelocked_balance),
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

pub async fn get_address(mut client: Client) -> McpResult<Value> {
    let response = client.get_address(Empty {}).await?.into_inner();

    Ok(json!({
        "addresses": {
            "interactive_address": hex::encode(&response.interactive_address),
            "one_sided_address": hex::encode(&response.one_sided_address),
        },
        "info": {
            "interactive_address_info": "Used for interactive Mimblewimble transactions (default)",
            "one_sided_address_info": "Used for one-sided transactions and stealth payments",
            "format": "Hexadecimal encoding of the address bytes",
        }
    }))
}

#[allow(clippy::indexing_slicing)]
pub async fn get_transaction_info(mut client: Client, params: &Value) -> McpResult<Value> {
    let transaction_ids: Vec<u64> = params
        .get("transaction_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_request("transaction_ids array is required"))?
        .iter()
        .filter_map(|v| v.as_u64())
        .collect();

    if transaction_ids.is_empty() {
        return Err(McpError::invalid_request("At least one transaction ID is required"));
    }

    let response = client
        .get_transaction_info(GetTransactionInfoRequest { transaction_ids })
        .await?
        .into_inner();
    #[allow(clippy::cast_possible_wrap)]
    let transactions: Vec<Value> = response
        .transactions
        .iter()
        .map(|tx| {
            json!({
                "tx_id": tx.tx_id,
                "source_address": hex::encode(&tx.source_address),
                "dest_address": hex::encode(&tx.dest_address),
                "status": match tx.status {
                    0 => "COMPLETED",
                    1 => "BROADCAST",
                    2 => "MINED_UNCONFIRMED",
                    3 => "IMPORTED",
                    4 => "PENDING",
                    5 => "COINBASE",
                    6 => "MINED_CONFIRMED",
                    7 => "REJECTED",
                    8 => "CANCELLED",
                    9 => "NOT_FOUND",
                    15 => "MINED_CONFIRMED_LOCKED",
                    16 => "ONE_SIDED_CONFIRMED_LOCKED",
                    17 => "COINBASE_CONFIRMED_LOCKED",
                    _ => "UNKNOWN",
                },
                "direction": match tx.direction {
                    0 => "UNKNOWN",
                    1 => "INBOUND",
                    2 => "OUTBOUND",
                    _ => "UNKNOWN",
                },
                "amount": tx.amount,
                "fee": tx.fee,
                "is_cancelled": tx.is_cancelled,
                "excess_sig": hex::encode(&tx.excess_sig),
                "timestamp": tx.timestamp,
                "payment_id": hex::encode(&tx.user_payment_id),
                "mined_in_block_height": tx.mined_in_block_height,
                "formatted": {
                    "amount_tari": (tx.amount as f64 / 1_000_000.0),
                    "fee_tari": (tx.fee as f64 / 1_000_000.0),
                    "timestamp_readable": if tx.timestamp > 0 {
                        chrono::DateTime::from_timestamp(tx.timestamp as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_else(|| "Invalid timestamp".to_string())
                    } else {
                        "No timestamp".to_string()
                    },
                },
                "confirmations": if tx.mined_in_block_height > 0 && tx.status == 6 {
                    "CONFIRMED"
                } else if tx.mined_in_block_height > 0 {
                    "MINED_UNCONFIRMED"
                } else {
                    "UNCONFIRMED"
                },
            })
        })
        .collect();

    let total_amount: u64 = transactions.iter().filter_map(|tx| tx["amount"].as_u64()).sum();
    let total_fees: u64 = transactions.iter().filter_map(|tx| tx["fee"].as_u64()).sum();
    let confirmed_count = transactions
        .iter()
        .filter(|tx| tx["status"].as_str() == Some("MINED_CONFIRMED"))
        .count();

    Ok(json!({
        "transactions": transactions,
        "summary": {
            "total_transactions": transactions.len(),
            "confirmed_transactions": confirmed_count,
            "pending_transactions": transactions.len().saturating_sub(confirmed_count),
            "total_amount": total_amount,
            "total_fees": total_fees,
            "total_amount_tari": (total_amount as f64 / 1_000_000.0),
            "total_fees_tari": (total_fees as f64 / 1_000_000.0),
        }
    }))
}

/// Parse a single `PaymentRecipient` out of the JSON supplied to the `transfer` tool.
fn parse_payment_recipient(index: usize, recipient_data: &Value) -> McpResult<PaymentRecipient> {
    let address = recipient_data
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_request(format!("address is required for recipient {index}")))?;

    let amount = recipient_data
        .get("amount")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request(format!("amount is required for recipient {index}")))?;

    if amount == 0 {
        return Err(McpError::invalid_request(format!(
            "amount must be greater than 0 for recipient {index}"
        )));
    }

    let fee_per_gram = recipient_data
        .get("fee_per_gram")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request(format!("fee_per_gram is required for recipient {index}")))?;

    let payment_type = recipient_data.get("payment_type").and_then(|v| v.as_u64()).unwrap_or(0); // Default to standard payment

    let raw_payment_id = recipient_data
        .get("payment_id")
        .and_then(|v| v.as_str())
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default();

    let user_payment_id = recipient_data
        .get("payment_id")
        .and_then(|v| v.as_str())
        .map(|payment_id_str| UserPaymentId {
            utf8_string: payment_id_str.to_string(),
            u256: vec![],
            user_bytes: vec![],
        });

    #[allow(clippy::cast_possible_truncation)]
    Ok(PaymentRecipient {
        address: address.to_string(),
        amount,
        fee_per_gram,
        payment_type: payment_type as i32,
        raw_payment_id,
        user_payment_id,
    })
}

#[allow(clippy::indexing_slicing)]
pub async fn transfer(mut client: Client, params: &Value) -> McpResult<Value> {
    let recipients_array = params
        .get("recipients")
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_request("recipients array is required"))?;

    if recipients_array.is_empty() {
        return Err(McpError::invalid_request("At least one recipient is required"));
    }

    let mut recipients = Vec::with_capacity(recipients_array.len());
    let mut total_amount = 0u64;

    for (i, recipient_data) in recipients_array.iter().enumerate() {
        let recipient = parse_payment_recipient(i, recipient_data)?;
        total_amount = total_amount.saturating_add(recipient.amount);
        recipients.push(recipient);
    }

    // 1 million Tari
    if total_amount > 1_000_000_000_000 {
        return Err(McpError::invalid_request("Total transfer amount exceeds safety limit"));
    }

    let response = client
        .transfer(TransferRequest {
            recipients,
            single_tx: false,
            excluded_commitments: vec![],
        })
        .await?
        .into_inner();

    let results: Vec<Value> = response
        .results
        .iter()
        .map(|result| {
            json!({
                "address": result.address,
                "transaction_id": result.transaction_id,
                "is_success": result.is_success,
                "failure_message": result.failure_message,
            })
        })
        .collect();

    let successful_transfers = results
        .iter()
        .filter(|r| r["is_success"].as_bool().unwrap_or(false))
        .count();

    let failed_transfers = results.len().saturating_sub(successful_transfers);

    Ok(json!({
        "results": results,
        "summary": {
            "total_recipients": results.len(),
            "successful_transfers": successful_transfers,
            "failed_transfers": failed_transfers,
            "total_amount": total_amount,
            "total_amount_tari": (total_amount as f64 / 1_000_000.0),
            "success_rate": if results.is_empty() {
                0.0
            }else {
                (successful_transfers as f64 / results.len() as f64 * 100.0).round()
            },
        },
        "transaction_ids": results.iter()
            .filter(|r| r["is_success"].as_bool().unwrap_or(false))
            .filter_map(|r| r["transaction_id"].as_u64())
            .collect::<Vec<_>>(),
    }))
}

pub async fn coin_split(mut client: Client, params: &Value) -> McpResult<Value> {
    let amount_per_split = get_required_u64_param(params, "amount_per_split")?;
    let split_count = get_required_u64_param(params, "split_count")?;
    let fee_per_gram = get_required_u64_param(params, "fee_per_gram")?;

    if amount_per_split == 0 {
        return Err(McpError::invalid_request("amount_per_split must be greater than 0"));
    }

    if split_count == 0 || split_count > 100 {
        return Err(McpError::invalid_request("split_count must be between 1 and 100"));
    }

    if fee_per_gram == 0 {
        return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
    }

    let lock_height = params.get("lock_height").and_then(|v| v.as_u64()).unwrap_or(0);

    let payment_id = if let Some(payment_id_str) = params.get("payment_id").and_then(|v| v.as_str()) {
        payment_id_str.as_bytes().to_vec()
    } else {
        vec![]
    };

    let total_amount = amount_per_split.saturating_mul(split_count);

    let response = client
        .coin_split(CoinSplitRequest {
            amount_per_split,
            split_count,
            fee_per_gram,
            lock_height,
            payment_id,
        })
        .await?
        .into_inner();

    Ok(json!({
        "transaction_id": response.tx_id,
        "split_details": {
            "amount_per_split": amount_per_split,
            "split_count": split_count,
            "total_amount": total_amount,
            "fee_per_gram": fee_per_gram,
            "lock_height": lock_height,
        },
        "formatted": {
            "amount_per_split_tari": (amount_per_split as f64 / 1_000_000.0),
            "total_amount_tari": (total_amount as f64 / 1_000_000.0),
        },
        "benefits": [
            "Improves transaction flexibility by creating multiple smaller UTXOs",
            "Enables parallel spending for faster transaction processing",
            "Reduces likelihood of change outputs in future transactions"
        ],
        "estimated_completion": "Transaction will be confirmed within the next few blocks"
    }))
}

pub async fn cancel_transaction(mut client: Client, params: &Value) -> McpResult<Value> {
    let tx_id = get_required_u64_param(params, "tx_id")?;

    let response = client
        .cancel_transaction(CancelTransactionRequest {
            tx_id,
            force_if_completed: false,
        })
        .await?
        .into_inner();

    let message = if response.is_success {
        "Transaction has been successfully cancelled".to_string()
    } else {
        format!("Cancellation failed: {}", response.failure_message)
    };

    Ok(json!({
        "tx_id": tx_id,
        "is_success": response.is_success,
        "failure_message": response.failure_message,
        "status": if response.is_success {
            "CANCELLED"
        } else {
            "CANCELLATION_FAILED"
        },
        "message": message,
        "note": if response.is_success {
            "Funds will be available for spending again shortly"
        } else {
            "Transaction may have already been mined or does not exist"
        }
    }))
}
