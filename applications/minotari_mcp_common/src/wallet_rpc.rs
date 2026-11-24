// Copyright 2025, The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Wallet gRPC calls shared by the manual wallet MCP tools and the auto-discovered tools.

use minotari_wallet_grpc_client::{
    WalletGrpcClient,
    grpc::{
        BlockHashHex,
        BlockHeight,
        CancelTransactionRequest,
        ClaimHtlcRefundRequest,
        ClaimShaAtomicSwapRequest,
        CoinSplitRequest,
        Empty,
        GetBalanceRequest,
        GetCompletedTransactionsRequest,
        GetPaymentIdAddressRequest,
        GetTransactionInfoRequest,
        PaymentRecipient,
        SendShaAtomicSwapRequest,
        SendShaAtomicSwapResponse,
        TransactionStatus,
        TransferRequest,
        UserPaymentId,
        payment_recipient::PaymentType,
    },
};
use serde_json::{Value, json};
use tonic::{Code, Status, transport::Channel};

use crate::{
    McpError,
    McpResult,
    clean_upstream_text,
    get_optional_string_param,
    get_required_string_param,
    get_required_u64_param,
    node_rpc::{MAX_STREAM_ITEMS, collect_limited, limit_param},
};

type Client = WalletGrpcClient<Channel>;

/// Default total, fees included, the MCP server may move out of the wallet per process: 1,000,000 Tari, in µT
pub const DEFAULT_SPEND_BUDGET: u64 = 1_000_000 * 1_000_000;
/// Default highest fee_per_gram, in µT/g, any fund-moving call may use
pub const DEFAULT_MAX_FEE_PER_GRAM: u64 = 10_000;

/// Limits on the wallet calls that move funds, shared by every tool of one server process. The budget is in memory
/// and resets when the server restarts.
pub struct SpendLimits {
    /// µT still allowed to leave the wallet through this server
    remaining: tokio::sync::Mutex<u64>,
    max_fee_per_gram: u64,
}

impl SpendLimits {
    pub fn new(budget: u64, max_fee_per_gram: u64) -> Self {
        Self {
            remaining: tokio::sync::Mutex::new(budget),
            max_fee_per_gram,
        }
    }

    fn check_fee(&self, fee_per_gram: u64) -> McpResult<()> {
        if fee_per_gram > self.max_fee_per_gram {
            return Err(McpError::invalid_request(format!(
                "fee_per_gram {fee_per_gram} is above this server's limit of {} µT/g",
                self.max_fee_per_gram
            )));
        }
        Ok(())
    }

    /// Run `call`, which may move up to `charge` µT (amount plus an upper bound on fees), only if the budget still
    /// covers it, and charge the budget unless the call clearly failed before anything was sent. The lock is held
    /// across the call so concurrent calls cannot both pass the check.
    async fn spend<T>(&self, charge: u64, call: impl Future<Output = Result<T, Status>>) -> McpResult<T> {
        let mut remaining = self.remaining.lock().await;
        if charge > *remaining {
            return Err(McpError::invalid_request(format!(
                "This could move {charge} µT including fees but only {} µT of this server's spend budget remains",
                *remaining
            )));
        }
        match call.await {
            Ok(result) => {
                *remaining = remaining.saturating_sub(charge);
                Ok(result)
            },
            Err(status) if failed_before_sending(&status) => Err(status.into()),
            Err(status) => {
                // The wallet may have sent before failing, so fail safe
                *remaining = remaining.saturating_sub(charge);
                Err(McpError::tool_execution_failed(format!(
                    "{}; {charge} µT was counted against the spend budget because funds may have left the wallet",
                    McpError::from(status)
                )))
            },
        }
    }
}

/// Whether a wallet error means the call was refused before any funds could have been sent. NotFound is not one: the
/// wallet returns it from transfers, claims and refunds after the transaction was already submitted.
fn failed_before_sending(status: &Status) -> bool {
    matches!(
        status.code(),
        Code::InvalidArgument |
            Code::FailedPrecondition |
            Code::PermissionDenied |
            Code::Unauthenticated |
            Code::ResourceExhausted |
            Code::Unimplemented
    )
}

// shortcut: conservative stand-ins for tari_transaction_components' WeightParams::v1 (kernel 10 g, input 8 g,
// output 53 g plus 1 g per 16 bytes of features and script), which this crate does not depend on; derive the bound
// from them if it ever does. Allows 64 inputs and 1 KiB of features and script per output, so it over-estimates.
const KERNEL_GRAMS: u64 = 10;
const INPUTS_GRAMS: u64 = 64 * 8;
const OUTPUT_GRAMS: u64 = 53 + 1024 / 16;

/// Upper bound on the fee of one transaction with `outputs` outputs at `fee_per_gram`
fn fee_bound(fee_per_gram: u64, outputs: u64) -> u64 {
    let grams = KERNEL_GRAMS
        .saturating_add(INPUTS_GRAMS)
        .saturating_add(outputs.saturating_mul(OUTPUT_GRAMS));
    fee_per_gram.saturating_mul(grams)
}

/// Most recipients one Transfer call may pay
const MAX_TRANSFER_RECIPIENTS: usize = 32;

/// A wallet transaction status as its proto name without the `TRANSACTION_STATUS_` prefix, e.g. `MINED_CONFIRMED`
fn transaction_status_name(status: i32) -> &'static str {
    TransactionStatus::try_from(status)
        .map(|s| s.as_str_name().trim_start_matches("TRANSACTION_STATUS_"))
        .unwrap_or("UNKNOWN")
}

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
                "status": transaction_status_name(tx.status),
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
    reject_unknown_keys(
        recipient_data,
        &["address", "amount", "fee_per_gram", "payment_type", "payment_id"],
        &format!("transfer recipient {index}"),
    )?;
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

    // The wallet's Transfer rejects STANDARD_MIMBLEWIMBLE, so default to one-sided stealth
    let payment_type = match recipient_data.get("payment_type") {
        None => PaymentType::OneSidedToStealthAddress as u64,
        Some(v) => v.as_u64().ok_or_else(|| {
            McpError::invalid_request(format!("payment_type must be an integer for recipient {index}"))
        })?,
    };
    if payment_type == PaymentType::StandardMimblewimble as u64 {
        return Err(McpError::invalid_request(format!(
            "payment_type 0 (STANDARD_MIMBLEWIMBLE) is not supported for recipient {index}; use 2 \
             (ONE_SIDED_TO_STEALTH_ADDRESS)"
        )));
    }

    // Sent only as user_payment_id: the wallet reads a non-empty raw_payment_id as an encoded memo instead
    let user_payment_id = match recipient_data.get("payment_id") {
        None => None,
        Some(v) => Some(UserPaymentId {
            utf8_string: v
                .as_str()
                .ok_or_else(|| McpError::invalid_request(format!("payment_id must be a string for recipient {index}")))?
                .to_string(),
            u256: vec![],
            user_bytes: vec![],
        }),
    };

    #[allow(clippy::cast_possible_truncation)]
    Ok(PaymentRecipient {
        address: address.to_string(),
        amount,
        fee_per_gram,
        payment_type: payment_type as i32,
        raw_payment_id: vec![],
        user_payment_id,
    })
}

/// The Transfer request for `params`, its total amount, and the most it can cost including fees
fn transfer_request(params: &Value, limits: &SpendLimits) -> McpResult<(TransferRequest, u64, u64)> {
    reject_unknown_keys(params, &["recipients"], "transfer")?;
    let recipients_array = params
        .get("recipients")
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_request("recipients array is required"))?;

    if recipients_array.is_empty() {
        return Err(McpError::invalid_request("At least one recipient is required"));
    }
    if recipients_array.len() > MAX_TRANSFER_RECIPIENTS {
        return Err(McpError::invalid_request(format!(
            "At most {MAX_TRANSFER_RECIPIENTS} recipients are allowed per transfer"
        )));
    }

    let mut recipients = Vec::with_capacity(recipients_array.len());
    let mut total_amount = 0u64;
    let mut charge = 0u64;

    for (i, recipient_data) in recipients_array.iter().enumerate() {
        let recipient = parse_payment_recipient(i, recipient_data)?;
        limits.check_fee(recipient.fee_per_gram)?;
        total_amount = total_amount.saturating_add(recipient.amount);
        // single_tx is false, so each recipient is paid in its own transaction with a payment and a change output
        charge = charge
            .saturating_add(recipient.amount)
            .saturating_add(fee_bound(recipient.fee_per_gram, 2));
        recipients.push(recipient);
    }

    let request = TransferRequest {
        recipients,
        single_tx: false,
        excluded_commitments: vec![],
    };
    Ok((request, total_amount, charge))
}

#[allow(clippy::indexing_slicing)]
pub async fn transfer(mut client: Client, params: &Value, limits: &SpendLimits) -> McpResult<Value> {
    let (request, total_amount, charge) = transfer_request(params, limits)?;
    let response = limits
        .spend(charge, async {
            client.transfer(request).await.map(tonic::Response::into_inner)
        })
        .await?;

    let results: Vec<Value> = response
        .results
        .iter()
        .map(|result| {
            json!({
                "address": result.address,
                "transaction_id": result.transaction_id,
                "is_success": result.is_success,
                "failure_message": clean_upstream_text(&result.failure_message),
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

/// Reject any key of `value` outside `allowed`, so a misspelt or unsupported argument is not silently dropped
fn reject_unknown_keys(value: &Value, allowed: &[&str], context: &str) -> McpResult<()> {
    if let Some(key) = value
        .as_object()
        .and_then(|o| o.keys().find(|k| !allowed.contains(&k.as_str())))
    {
        return Err(McpError::invalid_request(format!(
            "{key} is not supported by {context}"
        )));
    }
    Ok(())
}

/// The CoinSplit request for `params`. The wallet's CoinSplit ignores lock_height and payment_id, so they are
/// rejected rather than accepted and dropped.
fn coin_split_request(params: &Value) -> McpResult<CoinSplitRequest> {
    reject_unknown_keys(
        params,
        &["amount_per_split", "split_count", "fee_per_gram"],
        "the wallet's CoinSplit",
    )?;
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

    Ok(CoinSplitRequest {
        amount_per_split,
        split_count,
        fee_per_gram,
        lock_height: 0,
        payment_id: vec![],
    })
}

pub async fn coin_split(mut client: Client, params: &Value, limits: &SpendLimits) -> McpResult<Value> {
    let request = coin_split_request(params)?;
    let (amount_per_split, split_count, fee_per_gram) =
        (request.amount_per_split, request.split_count, request.fee_per_gram);
    limits.check_fee(fee_per_gram)?;
    let total_amount = amount_per_split.saturating_mul(split_count);
    // The split outputs plus change
    let charge = total_amount.saturating_add(fee_bound(fee_per_gram, split_count.saturating_add(1)));

    let response = limits
        .spend(charge, async {
            client.coin_split(request).await.map(tonic::Response::into_inner)
        })
        .await?;

    Ok(json!({
        "transaction_id": response.tx_id,
        "split_details": {
            "amount_per_split": amount_per_split,
            "split_count": split_count,
            "total_amount": total_amount,
            "fee_per_gram": fee_per_gram,
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

    let failure_message = clean_upstream_text(&response.failure_message);
    let message = if response.is_success {
        "Transaction has been successfully cancelled".to_string()
    } else {
        format!("Cancellation failed: {failure_message}")
    };

    Ok(json!({
        "tx_id": tx_id,
        "is_success": response.is_success,
        "failure_message": failure_message,
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

pub async fn get_unspent_amounts(mut client: Client) -> McpResult<Value> {
    let response = client.get_unspent_amounts(Empty {}).await?.into_inner();

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

pub async fn get_complete_address(mut client: Client) -> McpResult<Value> {
    let response = client.get_complete_address(Empty {}).await?.into_inner();

    Ok(json!({
        "addresses": {
            "interactive": {
                "bytes": hex::encode(&response.interactive_address),
                "base58": response.interactive_address_base58,
                "emoji": response.interactive_address_emoji,
            },
            "one_sided": {
                "bytes": hex::encode(&response.one_sided_address),
                "base58": response.one_sided_address_base58,
                "emoji": response.one_sided_address_emoji,
            }
        },
        "formats": {
            "bytes": "Raw address bytes in hexadecimal format",
            "base58": "Human-readable Base58 encoded address",
            "emoji": "User-friendly emoji representation for easy sharing",
        },
        "usage_recommendations": {
            "interactive_address": "Use for most transactions - enables interactive negotiation",
            "one_sided_address": "Use for stealth payments or when sender privacy is required",
            "base58_format": "Recommended for sharing with other users",
            "emoji_format": "Fun alternative for casual transactions",
        }
    }))
}

pub async fn get_payment_id_address(mut client: Client, params: &Value) -> McpResult<Value> {
    let payment_id_str = get_required_string_param(params, "payment_id")?;

    let payment_id = if let Some(stripped) = payment_id_str.strip_prefix("0x") {
        hex::decode(stripped).map_err(|e| McpError::invalid_request(format!("Invalid hex payment ID: {e}")))?
    } else {
        payment_id_str.as_bytes().to_vec()
    };

    let response = client
        .get_payment_id_address(GetPaymentIdAddressRequest { payment_id })
        .await?
        .into_inner();

    Ok(json!({
        "payment_id": payment_id_str,
        "addresses": {
            "interactive": {
                "bytes": hex::encode(&response.interactive_address),
                "base58": response.interactive_address_base58,
                "emoji": response.interactive_address_emoji,
            },
            "one_sided": {
                "bytes": hex::encode(&response.one_sided_address),
                "base58": response.one_sided_address_base58,
                "emoji": response.one_sided_address_emoji,
            }
        },
        "info": {
            "purpose": "These addresses are unique to the specified payment ID",
            "tracking": "Payments to these addresses will be associated with the payment ID",
            "privacy": "Each payment ID generates unique addresses for better privacy",
        },
        "usage": {
            "invoicing": "Use these addresses for specific invoices or payment requests",
            "reconciliation": "Easier to track and reconcile payments from different sources",
            "reporting": "Simplifies accounting and transaction categorization",
        }
    }))
}

#[allow(clippy::too_many_lines, clippy::indexing_slicing)]
pub async fn get_completed_transactions(mut client: Client, params: &Value) -> McpResult<Value> {
    let payment_id = get_optional_string_param(params, "payment_id").map(|payment_id_str| UserPaymentId {
        utf8_string: payment_id_str,
        u256: vec![],
        user_bytes: vec![],
    });

    let block_hash = get_optional_string_param(params, "block_hash").map(|hash| BlockHashHex { hash });
    let block_height = params
        .get("block_height")
        .and_then(|v| v.as_u64())
        .map(|h| BlockHeight { block_height: h });

    let limit = limit_param(params, 100, MAX_STREAM_ITEMS as u64);

    let has_payment_id = payment_id.is_some();
    let has_block_hash = block_hash.is_some();
    let has_block_height = block_height.is_some();

    let stream = client
        .get_completed_transactions(GetCompletedTransactionsRequest {
            payment_id,
            block_hash,
            block_height,
        })
        .await?
        .into_inner();
    #[allow(clippy::cast_possible_wrap)]
    let (transactions, truncated): (Vec<Value>, _) = collect_limited(stream, limit, |tx_response| {
        tx_response.transaction.as_ref().map(|transaction| {
            json!({
                "tx_id": transaction.tx_id,
                "source_address": hex::encode(&transaction.source_address),
                "dest_address": hex::encode(&transaction.dest_address),
                "status": transaction_status_name(transaction.status),
                "direction": match transaction.direction {
                    0 => "UNKNOWN",
                    1 => "INBOUND",
                    2 => "OUTBOUND",
                    _ => "UNKNOWN",
                },
                "amount": transaction.amount,
                "fee": transaction.fee,
                "timestamp": transaction.timestamp,
                "payment_id": hex::encode(&transaction.user_payment_id),
                "mined_in_block_height": transaction.mined_in_block_height,
                "formatted": {
                    "amount_tari": (transaction.amount as f64 / 1_000_000.0),
                    "fee_tari": (transaction.fee as f64 / 1_000_000.0),
                    "date": if transaction.timestamp > 0 {
                        chrono::DateTime::from_timestamp(transaction.timestamp as i64, 0)
                            .map(|dt| dt.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "Invalid date".to_string())
                    } else {
                        "No date".to_string()
                    },
                    "time": if transaction.timestamp > 0 {
                        chrono::DateTime::from_timestamp(transaction.timestamp as i64, 0)
                            .map(|dt| dt.format("%H:%M:%S").to_string())
                            .unwrap_or_else(|| "Invalid time".to_string())
                    } else {
                        "No time".to_string()
                    },
                },
            })
        })
    })
    .await?;

    let inbound_count = transactions
        .iter()
        .filter(|tx| tx["direction"].as_str() == Some("INBOUND"))
        .count();
    let outbound_count = transactions
        .iter()
        .filter(|tx| tx["direction"].as_str() == Some("OUTBOUND"))
        .count();

    let total_inbound: u64 = transactions
        .iter()
        .filter(|tx| tx["direction"].as_str() == Some("INBOUND"))
        .filter_map(|tx| tx["amount"].as_u64())
        .sum();
    let total_outbound: u64 = transactions
        .iter()
        .filter(|tx| tx["direction"].as_str() == Some("OUTBOUND"))
        .filter_map(|tx| tx["amount"].as_u64())
        .sum();
    let total_fees: u64 = transactions.iter().filter_map(|tx| tx["fee"].as_u64()).sum();
    #[allow(clippy::cast_possible_wrap)]
    let net_balance_change = (total_inbound as i64)
        .saturating_sub(total_outbound as i64)
        .saturating_sub(total_fees as i64);
    #[allow(clippy::cast_possible_wrap)]
    Ok(json!({
        "transactions": transactions,
        "summary": {
            "total_transactions": transactions.len(),
            "inbound_transactions": inbound_count,
            "outbound_transactions": outbound_count,
            "total_inbound_amount": total_inbound,
            "total_outbound_amount": total_outbound,
            "total_fees_paid": total_fees,
            "net_balance_change": net_balance_change,
        },
        "formatted_summary": {
            "total_inbound_tari": (total_inbound as f64 / 1_000_000.0),
            "total_outbound_tari": (total_outbound as f64 / 1_000_000.0),
            "total_fees_tari": (total_fees as f64 / 1_000_000.0),
            "net_change_tari": (net_balance_change as f64 / 1_000_000.0),
        },
        "metadata": {
            "limit_applied": limit,
            "results_truncated": truncated,
            "filters": {
                "payment_id": has_payment_id,
                "block_hash": has_block_hash,
                "block_height": has_block_height,
            }
        }
    }))
}

pub async fn send_sha_atomic_swap(mut client: Client, params: &Value, limits: &SpendLimits) -> McpResult<Value> {
    reject_unknown_keys(params, &["recipient"], "SendShaAtomicSwapTransaction")?;
    let recipient_data = params
        .get("recipient")
        .ok_or_else(|| McpError::invalid_request("recipient object is required"))?;
    reject_unknown_keys(
        recipient_data,
        &["address", "amount", "fee_per_gram"],
        "the atomic swap recipient",
    )?;

    let address = recipient_data
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_request("recipient.address is required"))?;

    let amount = recipient_data
        .get("amount")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request("recipient.amount is required"))?;

    let fee_per_gram = recipient_data
        .get("fee_per_gram")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request("recipient.fee_per_gram is required"))?;

    if amount == 0 {
        return Err(McpError::invalid_request("amount must be greater than 0"));
    }

    if fee_per_gram == 0 {
        return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
    }
    limits.check_fee(fee_per_gram)?;

    let recipient = PaymentRecipient {
        address: address.to_string(),
        amount,
        fee_per_gram,
        payment_type: PaymentType::OneSidedToStealthAddress.into(),
        raw_payment_id: vec![],
        user_payment_id: None,
    };

    // The HTLC output plus change
    let charge = amount.saturating_add(fee_bound(fee_per_gram, 2));
    let response = limits
        .spend(charge, async {
            client
                .send_sha_atomic_swap_transaction(SendShaAtomicSwapRequest {
                    recipient: Some(recipient),
                })
                .await
                .map(tonic::Response::into_inner)
        })
        .await?;
    let failure_message = clean_upstream_text(&response.failure_message);
    let (pre_image, output_hash) = swap_secrets(&response);

    Ok(json!({
        "transaction_id": response.transaction_id,
        "pre_image": pre_image,
        "output_hash": output_hash,
        "is_success": response.is_success,
        "failure_message": failure_message,
        "swap_details": {
            "recipient_address": address,
            "amount": amount,
            "fee_per_gram": fee_per_gram,
            "amount_tari": (amount as f64 / 1_000_000.0),
        },
        "security_info": {
            "pre_image_purpose": "Keep this pre-image secret until you want to claim the swap",
            "output_hash_usage": "The recipient needs this hash to identify the swap output",
            "claiming_process": "To claim the swap, the recipient must provide the pre-image that matches this hash",
        },
        "next_steps": if response.is_success {
            vec![
                "Share the output_hash with the recipient".to_string(),
                "Keep the pre_image secret until ready to reveal".to_string(),
                "Monitor the transaction for confirmation".to_string(),
                "The recipient can claim using the pre_image once revealed".to_string()
            ]
        } else {
            vec![
                format!("Fix the issue: {failure_message}"),
                "Check wallet balance and network connectivity".to_string(),
                "Retry the atomic swap operation".to_string()
            ]
        }
    }))
}

/// The swap's pre-image and output hash, which the wallet already sends as hex text
fn swap_secrets(response: &SendShaAtomicSwapResponse) -> (String, String) {
    (
        clean_upstream_text(&response.pre_image),
        clean_upstream_text(&response.output_hash),
    )
}

/// Validate a 32-byte hex output hash argument
fn output_hash_param(params: &Value) -> McpResult<String> {
    let output_hash = get_required_string_param(params, "output_hash")?;
    if !output_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(McpError::invalid_request("output_hash must be a valid hex string"));
    }
    if output_hash.len() != 64 {
        return Err(McpError::invalid_request(
            "output_hash must be 64 hex characters (32 bytes)",
        ));
    }
    Ok(output_hash)
}

fn fee_per_gram_param(params: &Value, limits: &SpendLimits) -> McpResult<u64> {
    let fee_per_gram = get_required_u64_param(params, "fee_per_gram")?;
    if fee_per_gram == 0 {
        return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
    }
    limits.check_fee(fee_per_gram)?;
    Ok(fee_per_gram)
}

pub async fn claim_sha_atomic_swap(mut client: Client, params: &Value, limits: &SpendLimits) -> McpResult<Value> {
    reject_unknown_keys(
        params,
        &["output_hash", "pre_image", "fee_per_gram"],
        "ClaimShaAtomicSwapTransaction",
    )?;
    let output_hash = output_hash_param(params)?;
    let pre_image = get_required_string_param(params, "pre_image")?;
    let fee_per_gram = fee_per_gram_param(params, limits)?;

    if !pre_image.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(McpError::invalid_request("pre_image must be a valid hex string"));
    }

    // A claim only costs its fee
    let response = limits
        .spend(fee_bound(fee_per_gram, 1), async {
            client
                .claim_sha_atomic_swap_transaction(ClaimShaAtomicSwapRequest {
                    output: output_hash.clone(),
                    pre_image: pre_image.clone(),
                    fee_per_gram,
                })
                .await
                .map(tonic::Response::into_inner)
        })
        .await?;

    let result = response
        .results
        .as_ref()
        .ok_or_else(|| McpError::tool_execution_failed("No results in claim response"))?;
    let failure_message = clean_upstream_text(&result.failure_message);

    Ok(json!({
        "claim_result": {
            "address": result.address,
            "transaction_id": result.transaction_id,
            "is_success": result.is_success,
            "failure_message": failure_message,
        },
        "claim_details": {
            "output_hash": output_hash,
            "pre_image_provided": pre_image,
            "fee_per_gram": fee_per_gram,
        },
        "status": if result.is_success {
            "CLAIMED_SUCCESSFULLY"
        } else {
            "CLAIM_FAILED"
        },
        "message": if result.is_success {
            "Atomic swap has been successfully claimed".to_string()
        } else {
            format!("Claim failed: {failure_message}")
        },
        "security_notes": {
            "pre_image_revealed": "The pre-image is now publicly visible on the blockchain",
            "transaction_finality": "This claim transaction must be confirmed to complete the swap",
            "fund_availability": "Claimed funds will be available once the transaction confirms",
        },
        "next_steps": if result.is_success {
            vec![
                "Wait for transaction confirmation",
                "Funds will appear in wallet balance",
                "Atomic swap is now complete"
            ]
        } else {
            vec![
                "Verify the pre-image matches the expected hash",
                "Check that the output hasn't already been claimed",
                "Ensure sufficient funds for transaction fee",
                "Retry with correct parameters"
            ]
        }
    }))
}

pub async fn claim_htlc_refund(mut client: Client, params: &Value, limits: &SpendLimits) -> McpResult<Value> {
    reject_unknown_keys(params, &["output_hash", "fee_per_gram"], "ClaimHtlcRefundTransaction")?;
    let output_hash = output_hash_param(params)?;
    let fee_per_gram = fee_per_gram_param(params, limits)?;

    // A refund only costs its fee
    let response = limits
        .spend(fee_bound(fee_per_gram, 1), async {
            client
                .claim_htlc_refund_transaction(ClaimHtlcRefundRequest {
                    output_hash: output_hash.clone(),
                    fee_per_gram,
                })
                .await
                .map(tonic::Response::into_inner)
        })
        .await?;

    let result = response
        .results
        .as_ref()
        .ok_or_else(|| McpError::tool_execution_failed("No results in refund response"))?;
    let failure_message = clean_upstream_text(&result.failure_message);

    Ok(json!({
        "refund_result": {
            "address": result.address,
            "transaction_id": result.transaction_id,
            "is_success": result.is_success,
            "failure_message": failure_message,
        },
        "refund_details": {
            "output_hash": output_hash,
            "fee_per_gram": fee_per_gram,
        },
        "status": if result.is_success {
            "REFUND_CLAIMED"
        } else {
            "REFUND_FAILED"
        },
        "message": if result.is_success {
            "HTLC refund has been successfully claimed".to_string()
        } else {
            format!("Refund claim failed: {failure_message}")
        },
        "htlc_info": {
            "purpose": "HTLC refunds allow recovery of funds when atomic swaps timeout",
            "timelock": "Refunds are only possible after the timelock period expires",
            "finality": "This refund transaction must be confirmed to recover the funds",
        },
        "next_steps": if result.is_success {
            vec![
                "Wait for refund transaction confirmation",
                "Funds will be returned to wallet balance",
                "The atomic swap opportunity has expired"
            ]
        } else {
            vec![
                "Check that the timelock period has expired",
                "Verify the output hash is correct",
                "Ensure the HTLC hasn't been claimed by the recipient",
                "Check wallet balance for transaction fees"
            ]
        }
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::indexing_slicing)]
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    /// A wallet client connected to a gRPC endpoint with no services. The wallet client has no lazy constructor, so
    /// tests serve one: every call that reaches it fails with Unimplemented, which is not McpError::NotImplemented.
    pub(crate) async fn unimplemented_wallet_client() -> Client {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_routes(tonic::service::Routes::default())
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );
        WalletGrpcClient::connect(&format!("http://{addr}")).await.unwrap()
    }

    #[tokio::test]
    async fn spend_budget_is_cumulative_and_charged_unless_refused_before_sending() {
        let limits = SpendLimits::new(100, DEFAULT_MAX_FEE_PER_GRAM);
        limits.spend(60, async { Ok(()) }).await.unwrap();
        let err = limits.spend(50, async { Ok(()) }).await.unwrap_err().to_string();
        assert!(err.contains("only 40 µT"), "{err}");

        // Refused before anything was sent: not charged
        let refused = limits.spend(10, async { Err::<(), _>(Status::invalid_argument("bad address")) });
        assert!(refused.await.is_err());
        assert_eq!(*limits.remaining.lock().await, 40);

        // Funds may have left: charged, and the error says so
        let lost = limits.spend(10, async { Err::<(), _>(Status::unavailable("connection reset")) });
        let err = lost.await.unwrap_err().to_string();
        assert!(err.contains("counted against the spend budget"), "{err}");
        assert_eq!(*limits.remaining.lock().await, 30);

        // NotFound can follow a submitted claim or refund, so it is charged too
        let after_submit = limits.spend(10, async { Err::<(), _>(Status::not_found("Transaction not found")) });
        assert!(after_submit.await.is_err());
        assert_eq!(*limits.remaining.lock().await, 20);
    }

    #[test]
    fn swap_output_hash_can_be_claimed_with() {
        let response = SendShaAtomicSwapResponse {
            pre_image: "ab".repeat(32),
            output_hash: "cd".repeat(32),
            ..Default::default()
        };
        let (pre_image, output_hash) = swap_secrets(&response);
        assert_eq!(pre_image, "ab".repeat(32));
        assert_eq!(
            output_hash_param(&json!({ "output_hash": output_hash })).unwrap(),
            "cd".repeat(32)
        );
    }

    #[tokio::test]
    async fn repeated_tiny_transfers_exhaust_the_budget_through_fees() {
        let params = json!({"recipients": [{"address": "addr", "amount": 1, "fee_per_gram": 100}]});
        let probe = SpendLimits::new(0, DEFAULT_MAX_FEE_PER_GRAM);
        let (_, total_amount, charge) = transfer_request(&params, &probe).unwrap();
        assert_eq!(total_amount, 1);
        assert_eq!(charge, 1 + fee_bound(100, 2));

        let limits = SpendLimits::new(charge * 3, DEFAULT_MAX_FEE_PER_GRAM);
        for _ in 0..3 {
            limits.spend(charge, async { Ok(()) }).await.unwrap();
        }
        assert!(limits.spend(charge, async { Ok(()) }).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_spends_cannot_both_pass_the_check() {
        let limits = SpendLimits::new(100, DEFAULT_MAX_FEE_PER_GRAM);
        let slow_call = || async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(())
        };
        let (first, second) = tokio::join!(limits.spend(70, slow_call()), limits.spend(70, slow_call()));
        assert!(first.is_ok() != second.is_ok(), "exactly one spend fits the budget");
    }

    #[tokio::test]
    async fn every_fund_moving_call_checks_the_budget_and_fee_ceiling() {
        let client = unimplemented_wallet_client().await;
        let limits = SpendLimits::new(100_000, 100);
        let over_budget = |err: McpError| err.to_string().contains("spend budget");
        let over_fee = |err: McpError| err.to_string().contains("above this server's limit");

        let transfer_params =
            |amount: u64, fee: u64| json!({"recipients": [{"address": "addr", "amount": amount, "fee_per_gram": fee}]});
        assert!(over_budget(
            transfer(client.clone(), &transfer_params(100_000, 1), &limits)
                .await
                .unwrap_err()
        ));
        assert!(over_fee(
            transfer(client.clone(), &transfer_params(1, 101), &limits)
                .await
                .unwrap_err()
        ));

        let swap =
            |amount: u64, fee: u64| json!({"recipient": {"address": "addr", "amount": amount, "fee_per_gram": fee}});
        assert!(over_budget(
            send_sha_atomic_swap(client.clone(), &swap(100_000, 1), &limits)
                .await
                .unwrap_err()
        ));
        assert!(over_fee(
            send_sha_atomic_swap(client.clone(), &swap(1, 101), &limits)
                .await
                .unwrap_err()
        ));

        // 4 outputs of 30,000 µT is 120,000 µT
        let split = json!({"amount_per_split": 30_000, "split_count": 4, "fee_per_gram": 1});
        assert!(over_budget(
            coin_split(client.clone(), &split, &limits).await.unwrap_err()
        ));

        let claim = |fee: u64| json!({"output_hash": "00".repeat(32), "pre_image": "00", "fee_per_gram": fee});
        let refund = |fee: u64| json!({"output_hash": "00".repeat(32), "fee_per_gram": fee});
        assert!(over_fee(
            claim_sha_atomic_swap(client.clone(), &claim(101), &limits)
                .await
                .unwrap_err()
        ));
        assert!(over_fee(
            claim_htlc_refund(client.clone(), &refund(101), &limits)
                .await
                .unwrap_err()
        ));
        // Claims and refunds are charged their fee bound
        let small = SpendLimits::new(fee_bound(1, 1), 100);
        assert!(over_budget(
            claim_sha_atomic_swap(client.clone(), &claim(2), &small)
                .await
                .unwrap_err()
        ));
        assert!(over_budget(
            claim_htlc_refund(client.clone(), &refund(2), &small).await.unwrap_err()
        ));

        // Within the limits the call reaches the wallet, which refuses it as unimplemented, so nothing is charged
        assert!(!over_budget(
            transfer(client.clone(), &transfer_params(1, 1), &limits)
                .await
                .unwrap_err()
        ));
        assert_eq!(*limits.remaining.lock().await, 100_000);
    }

    #[tokio::test]
    async fn fund_moving_calls_refuse_arguments_they_do_not_understand() {
        let client = unimplemented_wallet_client().await;
        let limits = SpendLimits::new(DEFAULT_SPEND_BUDGET, DEFAULT_MAX_FEE_PER_GRAM);
        let recipient = json!({"address": "addr", "amount": 1, "fee_per_gram": 1});

        let dry_run = json!({"recipients": [recipient], "dry_run": true});
        let err = transfer(client.clone(), &dry_run, &limits)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("dry_run is not supported"), "{err}");

        let mut bad_payment_id = recipient.clone();
        bad_payment_id["payment_id"] = json!(7);
        let err = transfer(client.clone(), &json!({"recipients": [bad_payment_id]}), &limits)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("payment_id must be a string"), "{err}");

        let mut bad_payment_type = recipient.clone();
        bad_payment_type["payment_type"] = json!("2");
        assert!(
            transfer(client.clone(), &json!({"recipients": [bad_payment_type]}), &limits)
                .await
                .is_err()
        );

        let too_many = json!({"recipients": vec![recipient.clone(); MAX_TRANSFER_RECIPIENTS + 1]});
        assert!(transfer(client.clone(), &too_many, &limits).await.is_err());

        let swap = json!({"recipient": recipient, "dry_run": true});
        assert!(send_sha_atomic_swap(client.clone(), &swap, &limits).await.is_err());
        let refund = json!({"output_hash": "00".repeat(32), "fee_per_gram": 1, "force": true});
        let err = claim_htlc_refund(client.clone(), &refund, &limits)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("force is not supported"), "{err}");
        assert_eq!(*limits.remaining.lock().await, DEFAULT_SPEND_BUDGET);
    }

    #[test]
    fn unsupported_arguments_are_rejected() {
        let message = json!({"address": "addr", "amount": 5, "fee_per_gram": 1, "message": "memo"});
        assert!(matches!(
            parse_payment_recipient(0, &message),
            Err(McpError::InvalidRequest(_))
        ));

        let split = json!({"amount_per_split": 10, "split_count": 2, "fee_per_gram": 1});
        assert_eq!(coin_split_request(&split).unwrap().split_count, 2);
        let mut with_lock = split.clone();
        with_lock["lock_height"] = json!(100);
        let err = coin_split_request(&with_lock).unwrap_err().to_string();
        assert!(
            err.contains("lock_height is not supported by the wallet's CoinSplit"),
            "{err}"
        );
        let mut with_payment_id = split;
        with_payment_id["payment_id"] = json!("x");
        assert!(coin_split_request(&with_payment_id).is_err());
    }

    #[test]
    fn transaction_status_names_follow_the_proto() {
        assert_eq!(transaction_status_name(6), "MINED_CONFIRMED");
        assert_eq!(transaction_status_name(8), "ONE_SIDED_UNCONFIRMED");
        assert_eq!(transaction_status_name(9), "ONE_SIDED_CONFIRMED");
        assert_eq!(transaction_status_name(-1), "UNKNOWN");
    }

    #[test]
    fn recipient_defaults_to_one_sided_stealth_and_sends_payment_id_once() {
        let recipient = parse_payment_recipient(
            0,
            &json!({"address": "addr", "amount": 5, "fee_per_gram": 1, "payment_id": "invoice-7"}),
        )
        .unwrap();
        assert_eq!(recipient.payment_type, PaymentType::OneSidedToStealthAddress as i32);
        assert!(recipient.raw_payment_id.is_empty());
        assert_eq!(recipient.user_payment_id.unwrap().utf8_string, "invoice-7");

        let standard = json!({"address": "addr", "amount": 5, "fee_per_gram": 1, "payment_type": 0});
        assert!(matches!(
            parse_payment_recipient(0, &standard),
            Err(McpError::InvalidRequest(_))
        ));
    }
}
