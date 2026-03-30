//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::time::Duration;

use minotari_app_grpc::tari_rpc::{self as grpc};
use minotari_wallet_grpc_client::WalletGrpcClient;
use tonic::transport::Channel;

use crate::polling::scaled_timeout;

/// Wait for a specific transaction to reach a target status by subscribing to the wallet's
/// transaction event stream instead of polling.
///
/// Falls back to polling via `GetTransactionInfo` if the stream is unavailable or the event
/// doesn't arrive within the timeout.
///
/// `target_status` should be one of: "Pending", "Completed", "Broadcast",
/// "Mined_or_OneSidedUnconfirmed", "Mined_or_OneSidedConfirmed", "Coinbase"
pub async fn wait_for_tx_status(
    client: &mut WalletGrpcClient<Channel>,
    tx_id: u64,
    target_status: &str,
    timeout: Duration,
) -> Result<(), String> {
    let timeout = scaled_timeout(timeout);
    let deadline = tokio::time::Instant::now() + timeout;

    // Try event stream first — if it's available, we get instant notifications.
    // Timeout the stream open quickly so we don't waste time if the RPC isn't supported.
    let stream_result = tokio::time::timeout(
        Duration::from_secs(5),
        client.stream_transaction_events(grpc::TransactionEventRequest {}),
    )
    .await
    .ok()
    .and_then(|r| r.ok());

    if let Some(response) = stream_result {
        let mut stream = response.into_inner();

        // First check current state in case it already matches
        if check_tx_status_matches(client, tx_id, target_status).await.unwrap_or(false) {
            return Ok(());
        }

        // Listen for events until timeout
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(format!(
                    "Timed out after {:.1}s waiting for tx {tx_id} to reach status '{target_status}' via event stream",
                    timeout.as_secs_f64()
                ));
            }

            let remaining = deadline.saturating_duration_since(now);
            match tokio::time::timeout(remaining, stream.message()).await {
                Ok(Ok(Some(event_response))) => {
                    if let Some(event) = event_response.transaction {
                        // Check if this event is for our transaction
                        if let Ok(event_tx_id) = event.tx_id.parse::<u64>() &&
                            event_tx_id == tx_id &&
                            status_matches_target(&event.status, target_status)
                        {
                            return Ok(());
                        }
                    }
                },
                Ok(Ok(None)) => {
                    // Stream ended — fall back to polling
                    break;
                },
                Ok(Err(_)) => {
                    // Stream error — fall back to polling
                    break;
                },
                Err(_) => {
                    // Timeout
                    return Err(format!(
                        "Timed out after {:.1}s waiting for tx {tx_id} to reach status '{target_status}'",
                        timeout.as_secs_f64()
                    ));
                },
            }
        }
    }

    // Fallback: poll via GetTransactionInfo (same as before but with exponential backoff)
    let mut interval = Duration::from_millis(250);
    let max_interval = Duration::from_secs(2);
    let mut last_error: Option<String> = None;

    loop {
        match check_tx_status_matches(client, tx_id, target_status).await {
            Ok(true) => return Ok(()),
            Ok(false) => {},
            Err(e) => {
                // gRPC errors are transient — keep retrying
                last_error = Some(e);
            },
        }

        if tokio::time::Instant::now() >= deadline {
            let current = get_current_tx_status(client, tx_id).await;
            let extra = last_error
                .map(|e| format!(", last error: {e}"))
                .unwrap_or_default();
            return Err(format!(
                "Timed out after {:.1}s waiting for tx {tx_id} to reach status '{target_status}' (current: {current}{extra})",
                timeout.as_secs_f64()
            ));
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(interval.min(remaining)).await;
        interval = Duration::from_secs_f64((interval.as_secs_f64() * 1.5).min(max_interval.as_secs_f64()));
    }
}

/// Check if the current transaction status matches the target.
async fn check_tx_status_matches(
    client: &mut WalletGrpcClient<Channel>,
    tx_id: u64,
    target_status: &str,
) -> Result<bool, String> {
    let request = grpc::GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };
    let tx_info = client
        .get_transaction_info(request)
        .await
        .map_err(|e| format!("gRPC error getting tx info for {tx_id}: {e}"))?
        .into_inner();

    let Some(tx) = tx_info.transactions.first() else {
        return Err(format!("No transaction info returned for tx_id {tx_id}"));
    };

    Ok(tx_status_matches(tx.status(), target_status))
}

/// Get the current status of a transaction as a string (for error messages).
async fn get_current_tx_status(client: &mut WalletGrpcClient<Channel>, tx_id: u64) -> String {
    let request = grpc::GetTransactionInfoRequest {
        transaction_ids: vec![tx_id],
    };
    match client.get_transaction_info(request).await {
        Ok(resp) => {
            let inner = resp.into_inner();
            inner
                .transactions
                .first()
                .map(|tx| format!("{:?}", tx.status()))
                .unwrap_or_else(|| "unknown".to_string())
        },
        Err(e) => format!("error: {e}"),
    }
}

/// Check if a gRPC TransactionStatus enum matches the string-based target.
fn tx_status_matches(status: grpc::TransactionStatus, target: &str) -> bool {
    match target {
        "Pending" => matches!(
            status,
            grpc::TransactionStatus::Pending |
                grpc::TransactionStatus::Completed |
                grpc::TransactionStatus::Broadcast |
                grpc::TransactionStatus::MinedUnconfirmed |
                grpc::TransactionStatus::MinedConfirmed |
                grpc::TransactionStatus::OneSidedUnconfirmed |
                grpc::TransactionStatus::OneSidedConfirmed |
                grpc::TransactionStatus::CoinbaseUnconfirmed |
                grpc::TransactionStatus::CoinbaseConfirmed
        ),
        "Completed" => matches!(
            status,
            grpc::TransactionStatus::Completed |
                grpc::TransactionStatus::Broadcast |
                grpc::TransactionStatus::MinedUnconfirmed |
                grpc::TransactionStatus::MinedConfirmed |
                grpc::TransactionStatus::OneSidedUnconfirmed |
                grpc::TransactionStatus::OneSidedConfirmed |
                grpc::TransactionStatus::CoinbaseUnconfirmed |
                grpc::TransactionStatus::CoinbaseConfirmed
        ),
        "Broadcast" => matches!(
            status,
            grpc::TransactionStatus::Broadcast |
                grpc::TransactionStatus::MinedUnconfirmed |
                grpc::TransactionStatus::MinedConfirmed |
                grpc::TransactionStatus::OneSidedUnconfirmed |
                grpc::TransactionStatus::OneSidedConfirmed |
                grpc::TransactionStatus::CoinbaseUnconfirmed |
                grpc::TransactionStatus::CoinbaseConfirmed
        ),
        "Mined_or_OneSidedUnconfirmed" => matches!(
            status,
            grpc::TransactionStatus::MinedUnconfirmed |
                grpc::TransactionStatus::MinedConfirmed |
                grpc::TransactionStatus::OneSidedUnconfirmed |
                grpc::TransactionStatus::OneSidedConfirmed |
                grpc::TransactionStatus::CoinbaseUnconfirmed |
                grpc::TransactionStatus::CoinbaseConfirmed
        ),
        "Mined_or_OneSidedConfirmed" => matches!(
            status,
            grpc::TransactionStatus::MinedConfirmed |
                grpc::TransactionStatus::OneSidedConfirmed |
                grpc::TransactionStatus::CoinbaseConfirmed
        ),
        "Coinbase" => matches!(
            status,
            grpc::TransactionStatus::CoinbaseConfirmed | grpc::TransactionStatus::CoinbaseUnconfirmed
        ),
        _ => false,
    }
}

/// Check if a string-based status (from TransactionEvent) matches the target.
fn status_matches_target(event_status: &str, target: &str) -> bool {
    match target {
        "Pending" => true, // Any status satisfies "at least Pending"
        "Completed" => !matches!(event_status, "Pending"),
        "Broadcast" => matches!(
            event_status,
            "Broadcast" |
                "MinedUnconfirmed" |
                "MinedConfirmed" |
                "OneSidedUnconfirmed" |
                "OneSidedConfirmed" |
                "CoinbaseUnconfirmed" |
                "CoinbaseConfirmed"
        ),
        "Mined_or_OneSidedUnconfirmed" => matches!(
            event_status,
            "MinedUnconfirmed" |
                "MinedConfirmed" |
                "OneSidedUnconfirmed" |
                "OneSidedConfirmed" |
                "CoinbaseUnconfirmed" |
                "CoinbaseConfirmed"
        ),
        "Mined_or_OneSidedConfirmed" => {
            matches!(
                event_status,
                "MinedConfirmed" | "OneSidedConfirmed" | "CoinbaseConfirmed"
            )
        },
        "Coinbase" => matches!(event_status, "CoinbaseConfirmed" | "CoinbaseUnconfirmed"),
        _ => false,
    }
}
