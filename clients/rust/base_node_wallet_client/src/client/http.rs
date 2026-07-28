// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::time::Instant;

use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    MicroMinotari,
    rpc::{
        MAX_ALLOWED_QUERY_SIZE,
        models,
        models::{
            BlockHeader,
            FeePerGramStat,
            GenerateKernelMerkleProofResponse,
            GetUtxosDeletedInfoResponse,
            GetUtxosMinedInfoResponse,
            SyncUtxosByBlockResponseV0,
            SyncUtxosByBlockResponseV1,
            TipInfoResponse,
            TxQueryResponse,
            TxSubmissionResponse,
        },
    },
    transaction_components::{Transaction, TransactionOutput},
};
use tari_utilities::hex::{Hex, to_hex};
use tokio::sync::{RwLock, mpsc};
use url::Url;

use crate::{BaseNodeWalletClient, JsonRpcResponse};

const LOG_TARGET: &str = "tari::wallet::client::http";

/// The base node rejects any batch query carrying more than `MAX_ALLOWED_QUERY_SIZE` items with a `400`. Fail here
/// instead, so callers get an actionable error naming the limit rather than an opaque HTTP error body.
fn check_query_size(len: usize, item_name: &str) -> Result<(), anyhow::Error> {
    if len > MAX_ALLOWED_QUERY_SIZE {
        return Err(anyhow!(
            "Cannot query {len} {item_name} in a single request, the base node allows at most \
             {MAX_ALLOWED_QUERY_SIZE}. Split the request into smaller batches."
        ));
    }
    Ok(())
}

/// HTTP client for the Base Node wallet service.
pub struct Client {
    local_api_address: Url,
    default_seed_address: Url,
    http_client: reqwest::Client,
    last_latency: RwLock<Option<(std::time::Duration, Instant)>>,
    use_local_api_address: RwLock<Option<bool>>,
}

impl Client {
    pub fn new(local_api_address: Url, default_seed_address: Url) -> Self {
        let http_client_builder = reqwest::Client::builder();
        let http_client = http_client_builder
            .http2_initial_stream_window_size(4 * 1024 * 1024)
            .build()
            .expect("http2 init");
        Self {
            local_api_address,
            default_seed_address,
            http_client,
            last_latency: RwLock::new(None),
            use_local_api_address: RwLock::new(None),
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            local_api_address: self.local_api_address.clone(),
            default_seed_address: self.default_seed_address.clone(),
            http_client: self.http_client.clone(),
            last_latency: RwLock::new(None),
            use_local_api_address: RwLock::new(None),
        }
    }
}
impl Client {
    async fn set_last_latency(&self, duration: std::time::Duration) {
        let mut last_latency = self.last_latency.write().await;
        *last_latency = Some((duration, Instant::now()));
    }

    /// returns the Url of the https server to use
    async fn http_server_address(&self) -> Result<&Url, anyhow::Error> {
        if let Some(use_local) = self.use_local_api_address.read().await.as_ref() {
            if *use_local {
                return Ok(&self.local_api_address);
            } else {
                return Ok(&self.default_seed_address);
            }
        }
        debug!(
            target: LOG_TARGET, "There is no last connected server set, trying local API address: {}",
            self.local_api_address
        );
        // Try to reach the local API address
        let res = match self
            .http_client
            .get(self.local_api_address.join("/get_tip_info")?)
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                debug!(target: LOG_TARGET, "Failed to reach local API address {}: {}", self.local_api_address, e);
                *self.use_local_api_address.write().await = Some(false);
                return Ok(&self.default_seed_address);
            },
        };
        if res.status().is_client_error() || res.status().is_server_error() {
            debug!(
                target: LOG_TARGET, "Local API address {} is not reachable, falling back to default seed address: {}",
                self.local_api_address, self.default_seed_address
            );
            // we cant use the local, use the default seed address
            *self.use_local_api_address.write().await = Some(false);
            Ok(&self.default_seed_address)
        } else {
            debug!(target: LOG_TARGET, "Using local API address: {}", self.local_api_address);
            // if we can reach the local api, then use it
            *self.use_local_api_address.write().await = Some(true);
            Ok(&self.local_api_address)
        }
    }

    async fn generate_request_url(&self, path: &str, query: &[(&str, String)]) -> Result<Url, anyhow::Error> {
        let base_url = self.http_server_address().await?;
        let mut url = base_url.join(path)?;
        if !query.is_empty() {
            let query_string: String = query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<String>>()
                .join("&");
            url.set_query(Some(&query_string));
        }
        Ok(url)
    }

    async fn send_get_request<T: DeserializeOwned>(
        &self,
        path: &str,
        query_params: &[(&str, String)],
    ) -> Result<T, anyhow::Error> {
        let timer = Instant::now();
        let url = self.generate_request_url(path, query_params).await?;
        let res = self.http_client.get(url).send().await?;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        let resp = res.json().await?;
        self.set_last_latency(timer.elapsed()).await;
        Ok(resp)
    }
}

#[async_trait]
impl BaseNodeWalletClient for Client {
    async fn get_address(&self) -> String {
        match self.http_server_address().await {
            Ok(v) => v.to_string(),
            _ => "".to_string(),
        }
    }

    async fn is_online(&self) -> bool {
        self.get_tip_info().await.is_ok()
    }

    async fn get_tip_info(&self) -> Result<TipInfoResponse, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(target: LOG_TARGET, "Requesting tip info from Base Node wallet service at {}", server_address);
        let timer = Instant::now();

        let res = self
            .http_client
            .get(server_address.join("/get_tip_info")?)
            .send()
            .await?;
        self.set_last_latency(timer.elapsed()).await;

        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ))
        } else {
            Ok(res.json::<TipInfoResponse>().await?)
        }
    }

    async fn get_header_by_height(&self, height: u64) -> Result<Option<BlockHeader>, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting block header at height {height} from Base Node wallet service at {server_address}"
        );
        let mut target_url = server_address.join("/get_header_by_height")?;
        target_url.set_query(Some(format!("height={height}").as_str()));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status() == StatusCode::NOT_FOUND {
            debug!(
                target: LOG_TARGET,
                "No block header found at height {height} from Base Node wallet service at {server_address}"
            );
            return Ok(None);
        }
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        } else {
            let text = res.text().await?;
            match serde_json::from_str::<BlockHeader>(&text) {
                Ok(header) => Ok(Some(header)),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Error decoding block header at height {height}: {e}, Received:{text}");
                    Err(anyhow!("Error decoding block header at height {height}: {e}"))
                },
            }
        }
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET, "Requesting block height at epoch time {epoch_time} from Base Node wallet service at {server_address}"
        );
        let mut target_url = server_address.join("/get_height_at_time")?;
        target_url.set_query(Some(format!("time={epoch_time}").as_str()));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ))
        } else {
            Ok(res.json::<u64>().await?)
        }
    }

    async fn get_utxos_by_block(
        &self,
        header_hash: Vec<u8>,
    ) -> Result<tari_transaction_components::rpc::models::GetUtxosByBlockResponse, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting UTXOs for block with header hash {} from Base Node wallet service at {}",
            header_hash.to_hex(), server_address
        );
        let mut target_url = server_address.join("/get_utxos_by_block")?;
        target_url.set_query(Some(&format!("header_hash={}", header_hash.to_hex())));
        let timer = Instant::now();
        let res = self
            .http_client
            .get(target_url)
            .json(&models::GetUtxosByBlockRequest { header_hash })
            .send()
            .await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        Ok(res.json::<models::GetUtxosByBlockResponse>().await?)
    }

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        shutdown: ShutdownSignal,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponseV0, anyhow::Error>>, anyhow::Error> {
        debug!(
            target: LOG_TARGET,
            "Starting UTXO sync from {}",
            start_header_hash.to_hex()
        );
        let mut target_url = self.http_server_address().await?.join("/sync_utxos_by_block")?;
        let (resp_tx, resp_rx) = mpsc::channel(1000);
        let start_header_hash_hex = start_header_hash.to_hex();
        let client = self.http_client.clone();

        let limit = 25;
        tokio::spawn(async move {
            let mut page = 0;
            let mut has_next_page = true;
            while has_next_page {
                if shutdown.is_triggered() {
                    info!(target: LOG_TARGET, "UTXO sync task shutdown triggered, exiting loop");
                    break;
                }
                target_url.set_query(Some(
                    format!("start_header_hash={start_header_hash_hex}&limit={limit}&page={page}&version=1").as_str(),
                ));
                debug!(target: LOG_TARGET, "Requesting UTXOs by block from Base Node wallet service at {target_url}");
                match client.get(target_url.clone()).send().await {
                    Ok(response) => match response.json::<SyncUtxosByBlockResponseV1>().await {
                        Ok(response) => {
                            has_next_page = response.has_next_page;
                            debug!(target: LOG_TARGET, "Received UTXOs for page {page}");
                            if let Err(send_error) = resp_tx.send(Ok(response.into())).await {
                                error!(target: LOG_TARGET, "Error sending utxo response: {send_error:?}");
                            }
                        },
                        Err(error) => {
                            if let Err(send_error) = resp_tx.send(Err(error.into())).await {
                                error!(target: LOG_TARGET, "Error sending error result: {send_error:?}");
                            }
                            break;
                        },
                    },
                    Err(error) => {
                        if let Err(send_error) = resp_tx.send(Err(error.into())).await {
                            error!(target: LOG_TARGET, "Error sending error result: {send_error:?}");
                        }
                        break;
                    },
                }

                if has_next_page {
                    page += 1;
                }
            }
        });

        Ok(resp_rx)
    }

    async fn get_last_request_latency(&self) -> Option<std::time::Duration> {
        self.last_latency.read().await.map(|(duration, _)| duration)
    }

    async fn get_utxos_mined_info(
        &self,
        hashes: Vec<Vec<u8>>,
        version: u32,
    ) -> Result<GetUtxosMinedInfoResponse, anyhow::Error> {
        check_query_size(hashes.len(), "output hashes")?;
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting matching UTXOs (version={}) for {} hashes from Base Node wallet service at {}",
            version, hashes.len(), server_address
        );
        let mut target_url = server_address.join("/get_utxos_mined_info")?;
        target_url.set_query(Some(&format!(
            "hashes={}&version={}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(","),
            version
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        debug!(
            target: LOG_TARGET,
            "Received UTXOs mined info for {} hashes from Base Node wallet service at {}",
            hashes.len(), server_address
        );

        let res_text = res.text().await?;
        let json = serde_json::from_str::<GetUtxosMinedInfoResponse>(&res_text)
            .map_err(|e| anyhow!("Failed to parse response JSON: {e}"))?;
        debug!(target: LOG_TARGET, "Response json: {json}");
        Ok(json)
    }

    async fn query_deleted_utxos(
        &self,
        hashes: Vec<Vec<u8>>,
        must_include_header: Vec<u8>,
    ) -> Result<GetUtxosDeletedInfoResponse, anyhow::Error> {
        check_query_size(hashes.len(), "output hashes")?;
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting deleted UTXOs for {} hashes, must include header {} from Base Node wallet",
            hashes.len(), must_include_header.to_hex()
        );
        let mut target_url = server_address.join("/get_utxos_deleted_info")?;
        target_url.set_query(Some(&format!(
            "hashes={}&must_include_header={}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(","),
            must_include_header.to_hex()
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        debug!(
            target: LOG_TARGET,
            "Received deleted UTXOs for {} hashes from Base Node wallet service at {}",
            hashes.len(), server_address
        );
        let res_text = res.text().await?;
        let json = serde_json::from_str::<GetUtxosDeletedInfoResponse>(&res_text)
            .map_err(|e| anyhow!("Failed to parse response JSON: {e}"))?;
        Ok(json)
    }

    async fn fetch_utxo(&self, utxo: Vec<u8>) -> Result<Option<TransactionOutput>, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting UTXO with hash {} from Base Node wallet service at {}",
            utxo.to_hex(), server_address
        );
        let mut target_url = server_address.join("/fetch_utxo")?;
        target_url.set_query(Some(&format!("utxo={}", utxo.to_hex())));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        Ok(res.json::<Option<TransactionOutput>>().await?)
    }

    async fn submit_transaction(&self, transaction: Transaction) -> Result<TxSubmissionResponse, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(target: LOG_TARGET, "Submitting transaction to Base Node wallet service at {server_address}");
        let target_url = server_address.join("/json_rpc")?;
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "submit_transaction",
            "params": {
                "transaction": transaction,
                "version": 2,
            }
        });

        let body_bytes = serde_json::to_vec(&request_body)?;
        let len = body_bytes.len();
        debug!(
            target: LOG_TARGET,
            "submit_transaction JSON body size: {}, bytes: ~{:.2} MiB, inputs: {}, outputs: {}",
            len, len as f64 / (1024.0 * 1024.0), transaction.body.inputs().len(), transaction.body.outputs().len()
        );

        let res = self.http_client.post(target_url).json(&request_body).send().await?;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        info!(target: LOG_TARGET, "Transaction submitted successfully to Base Node wallet service at {server_address}");
        let response = res.json::<JsonRpcResponse<TxSubmissionResponse>>().await?;
        match response.result {
            Some(result) => {
                debug!(target: LOG_TARGET, "Transaction submission response: {result:?}");
                Ok(result)
            },
            None => {
                let error_message = response.error.unwrap_or_else(|| "Unknown error".to_string());
                warn!(target: LOG_TARGET, "Transaction submission failed: {error_message}");
                Err(anyhow!("Transaction submission failed: {error_message}"))
            },
        }
    }

    async fn transaction_query(
        &self,
        excess_sig_nonce: Vec<u8>,
        excess_sig_sig: Vec<u8>,
    ) -> Result<TxQueryResponse, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Querying transaction with excess signature nonce {} and signature {}",
            excess_sig_nonce.to_hex(), excess_sig_sig.to_hex()
        );
        let mut target_url = server_address.join("/transactions")?;
        target_url.set_query(Some(&format!(
            "excess_sig_nonce={}&excess_sig_sig={}",
            excess_sig_nonce.to_hex(),
            excess_sig_sig.to_hex()
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }
        debug!(
            target: LOG_TARGET,
            "Transaction query successful for excess signature nonce {} and signature {}",
            excess_sig_nonce.to_hex(), excess_sig_sig.to_hex()
        );
        let response = res.json::<TxQueryResponse>().await?;
        Ok(response)
    }

    async fn get_mempool_fee_per_gram_stats(&self, count: u64) -> Result<FeePerGramStat, anyhow::Error> {
        let server_address = self.http_server_address().await?;
        debug!(
            target: LOG_TARGET,
            "Requesting mempool fee per gram stats with count {} from Base Node wallet service at {}",
            count, server_address
        );

        let mut target_url = server_address.join("/get_mempool_fee_per_gram_stats")?;
        target_url.set_query(Some(format!("count={count}").as_str()));

        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed()).await;

        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {status}. {body}");
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {status}. {body}"
            ));
        }

        #[derive(serde::Deserialize)]
        struct FeePerGramStatResponse {
            order: u64,
            min_fee_per_gram: u64,
            avg_fee_per_gram: u64,
            max_fee_per_gram: u64,
        }

        #[derive(serde::Deserialize)]
        struct GetMempoolFeePerGramStatsResponse {
            stats: Vec<FeePerGramStatResponse>,
        }

        let response = res.json::<GetMempoolFeePerGramStatsResponse>().await?;

        // Return the first stat or a default if empty
        let stat = response
            .stats
            .into_iter()
            .next()
            .map(|s| FeePerGramStat {
                order: s.order,
                min_fee_per_gram: MicroMinotari::from(s.min_fee_per_gram),
                avg_fee_per_gram: MicroMinotari::from(s.avg_fee_per_gram),
                max_fee_per_gram: MicroMinotari::from(s.max_fee_per_gram),
            })
            .unwrap_or_else(|| FeePerGramStat {
                order: 0,
                min_fee_per_gram: MicroMinotari::from(1),
                avg_fee_per_gram: MicroMinotari::from(1),
                max_fee_per_gram: MicroMinotari::from(1),
            });

        Ok(stat)
    }

    async fn get_kernel_merkle_proof(
        &self,
        excess_sig_nonce: &[u8],
        excess_sig: &[u8],
    ) -> Result<GenerateKernelMerkleProofResponse, anyhow::Error> {
        let resp = self
            .send_get_request("/generate_kernel_merkle_proof", &[
                ("excess_sig_public_nonce", to_hex(excess_sig_nonce)),
                ("excess_sig_signature", to_hex(excess_sig)),
            ])
            .await?;

        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_query_size_rejects_only_above_the_limit() {
        check_query_size(0, "output hashes").expect("empty query is not this check's concern");
        check_query_size(MAX_ALLOWED_QUERY_SIZE, "output hashes").expect("a query at the limit is allowed");

        let err = check_query_size(MAX_ALLOWED_QUERY_SIZE + 1, "output hashes")
            .expect_err("a query over the limit must be rejected before it is sent");
        assert!(err.to_string().contains(&MAX_ALLOWED_QUERY_SIZE.to_string()));
    }
}
