// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use std::{sync::RwLock, time::Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use reqwest::StatusCode;
use tari_core::{
    base_node::rpc::models::{
        self,
        BlockHeader,
        GetUtxosDeletedInfoResponse,
        GetUtxosMinedInfoResponse,
        SyncUtxosByBlockResponse,
        TipInfoResponse,
        TxQueryResponse,
        TxSubmissionResponse,
    },
    mempool::FeePerGramStat,
    transactions::{tari_amount::MicroMinotari, transaction_components::TransactionOutput},
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::sync::mpsc;
use url::Url;

use crate::{BaseNodeWalletClient, JsonRpcResponse};

const LOG_TARGET: &str = "tari::wallet::client::http";

/// HTTP client for the Base Node wallet service.
pub struct Client {
    api_address: Url,
    http_client: reqwest::Client,
    last_latency: RwLock<Option<(std::time::Duration, Instant)>>,
}

impl Client {
    pub fn new(api_address: Url) -> Self {
        Self {
            api_address,
            http_client: reqwest::Client::new(),
            last_latency: RwLock::new(None),
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            api_address: self.api_address.clone(),
            http_client: self.http_client.clone(),
            last_latency: RwLock::new(None),
        }
    }
}
impl Client {
    fn set_last_latency(&self, duration: std::time::Duration) {
        let mut last_latency = self.last_latency.write().expect("rwlock poisoned");
        *last_latency = Some((duration, Instant::now()));
    }
}

#[async_trait]
impl BaseNodeWalletClient for Client {
    fn get_address(&self) -> String {
        self.api_address.to_string()
    }

    fn is_online(&self) -> bool {
        self.last_latency
            .read()
            .expect("rwlock poisoned")
            .map(|latency| latency.1.elapsed() < std::time::Duration::from_secs(60))
            .unwrap_or(false)
    }

    async fn get_tip_info(&self) -> Result<TipInfoResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting tip info from Base Node wallet service at {}", self.api_address);
        let timer = Instant::now();
        let res = self
            .http_client
            .get(self.api_address.join("/get_tip_info")?)
            .send()
            .await?;
        self.set_last_latency(timer.elapsed());

        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
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
        debug!(target: LOG_TARGET, "Requesting block header at height {} from Base Node wallet service at {}", height, self.api_address);
        let mut target_url = self.api_address.join("/get_header_by_height")?;
        target_url.set_query(Some(format!("height={}", height).as_str()));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status() == StatusCode::NOT_FOUND {
            debug!(target: LOG_TARGET, "No block header found at height {} from Base Node wallet service at {}", height, self.api_address);
            return Ok(None);
        }
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        } else {
            let text = res.text().await?;
            match serde_json::from_str::<BlockHeader>(&text) {
                Ok(header) => {
                    // debug!(target: LOG_TARGET, "Received block header at height {}: {:?}", height, header);
                    Ok(Some(header))
                },
                Err(e) => {
                    warn!(target: LOG_TARGET, "Error decoding block header at height {}: {}, Received:{}", height, e, text);
                    Err(anyhow!("Error decoding block header at height {}: {}", height, e))
                },
            }
        }
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting block height at epoch time {} from Base Node wallet service at {}", epoch_time, self.api_address);
        let mut target_url = self.api_address.join("/get_height_at_time")?;
        target_url.set_query(Some(format!("time={}", epoch_time).as_str()));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ))
        } else {
            Ok(res.json::<u64>().await?)
        }
    }

    async fn get_utxos_by_block(&self, header_hash: Vec<u8>) -> Result<models::GetUtxosByBlockResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting UTXOs for block with header hash {} from Base Node wallet service at {}", header_hash.to_hex(), self.api_address);
        let mut target_url = self.api_address.join("/get_utxos_by_block")?;
        target_url.set_query(Some(&format!("header_hash={}", header_hash.to_hex())));
        let timer = Instant::now();
        let res = self
            .http_client
            .get(target_url)
            .json(&models::GetUtxosByBlockRequest { header_hash })
            .send()
            .await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body,
            ));
        }
        Ok(res.json::<models::GetUtxosByBlockResponse>().await?)
    }

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        end_header_hash: Vec<u8>,
        shutdown: ShutdownSignal,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponse, anyhow::Error>>, anyhow::Error> {
        debug!(target: LOG_TARGET, "Starting UTXO sync from {} to {}", start_header_hash.to_hex(), end_header_hash.to_hex());
        let mut target_url = self.api_address.join("/sync_utxos_by_block")?;
        let (resp_tx, resp_rx) = mpsc::channel(1000);
        let start_header_hash_hex = start_header_hash.to_hex();
        let end_header_hash_hex = end_header_hash.to_hex();
        let client = self.http_client.clone();

        let limit = 10;
        tokio::spawn(async move {
            let mut page = 0;
            let mut has_next_page = true;
            while has_next_page {
                if shutdown.is_triggered() {
                    info!(target: LOG_TARGET, "UTXO sync task shutdown triggered, exiting loop");
                    break;
                }
                target_url.set_query(Some(
                    format!(
                        "start_header_hash={}&end_header_hash={}&limit={}&page={}",
                        &start_header_hash_hex, &end_header_hash_hex, limit, page
                    )
                    .as_str(),
                ));
                debug!(target: LOG_TARGET, "Requesting UTXOs by block from Base Node wallet service at {}", target_url);
                match client.get(target_url.clone()).send().await {
                    Ok(response) => match response.json::<SyncUtxosByBlockResponse>().await {
                        Ok(response) => {
                            has_next_page = response.has_next_page;
                            debug!(target: LOG_TARGET, "Received UTXOs for page {}", page);
                            if let Err(send_error) = resp_tx.send(Ok(response)).await {
                                error!(target: LOG_TARGET, "Error sending utxo response: {:?}", send_error);
                            }
                        },
                        Err(error) => {
                            if let Err(send_error) = resp_tx.send(Err(error.into())).await {
                                error!(target: LOG_TARGET, "Error sending error result: {:?}", send_error);
                            }
                            break;
                        },
                    },
                    Err(error) => {
                        if let Err(send_error) = resp_tx.send(Err(error.into())).await {
                            error!(target: LOG_TARGET, "Error sending error result: {:?}", send_error);
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

    fn get_last_request_latency(&self) -> Option<std::time::Duration> {
        self.last_latency
            .read()
            .expect("rwlock poisoned")
            .map(|(duration, _)| duration)
    }

    async fn get_utxos_mined_info(&self, hashes: Vec<Vec<u8>>) -> Result<GetUtxosMinedInfoResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting matching UTXOs for hashes {:?} from Base Node wallet service at {}", hashes, self.api_address);
        let mut target_url = self.api_address.join("/get_utxos_mined_info")?;
        target_url.set_query(Some(&format!(
            "hashes={}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(",")
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        }
        info!(target: LOG_TARGET, "Received UTXOs mined info for hashes {:?} from Base Node wallet service at {}", hashes, self.api_address);

        let res_text = res.text().await?;
        debug!(target: LOG_TARGET, "Response text: {}", res_text);
        let json = serde_json::from_str::<GetUtxosMinedInfoResponse>(&res_text)
            .map_err(|e| anyhow!("Failed to parse response JSON: {}", e))?;
        Ok(json)
    }

    async fn query_deleted_utxos(
        &self,
        hashes: Vec<Vec<u8>>,
        must_include_header: Vec<u8>,
    ) -> Result<GetUtxosDeletedInfoResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting deleted UTXOs for hashes {:?}, must include:{:?} from Base Node wallet", hashes, &must_include_header);
        let mut target_url = self.api_address.join("/get_utxos_deleted_info")?;
        target_url.set_query(Some(&format!(
            "hashes={}&must_include_header={}",
            hashes.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(","),
            must_include_header.to_hex()
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        }
        info!(target: LOG_TARGET, "Received deleted UTXOs for hashes {:?} from Base Node wallet service at {}", hashes, self.api_address);
        let res_text = res.text().await?;
        debug!(target: LOG_TARGET, "Response text: {}", res_text);
        let json = serde_json::from_str::<GetUtxosDeletedInfoResponse>(&res_text)
            .map_err(|e| anyhow!("Failed to parse response JSON: {}", e))?;
        Ok(json)
    }

    async fn fetch_utxo(&self, utxo: Vec<u8>) -> Result<Option<TransactionOutput>, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting UTXO with hash {} from Base Node wallet service at {}", utxo.to_hex(), self.api_address);
        let mut target_url = self.api_address.join("/fetch_utxo")?;
        target_url.set_query(Some(&format!("utxo={}", utxo.to_hex())));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        }
        Ok(res.json::<Option<TransactionOutput>>().await?)
    }

    async fn submit_transaction(
        &self,
        transaction: tari_core::transactions::transaction_components::Transaction,
    ) -> Result<TxSubmissionResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Submitting transaction to Base Node wallet service at {}", self.api_address);
        let target_url = self.api_address.join("/json_rpc")?;
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "submit_transaction",
            "params": {
                "transaction": transaction,
            }
        });

        let res = self.http_client.post(target_url).json(&request_body).send().await?;
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        }
        info!(target: LOG_TARGET, "Transaction submitted successfully to Base Node wallet service at {}", self.api_address);
        let response = res.json::<JsonRpcResponse<TxSubmissionResponse>>().await?;
        match response.result {
            Some(result) => {
                debug!(target: LOG_TARGET, "Transaction submission response: {:?}", result);
                Ok(result)
            },
            None => {
                let error_message = response.error.unwrap_or_else(|| "Unknown error".to_string());
                warn!(target: LOG_TARGET, "Transaction submission failed: {}", error_message);
                Err(anyhow!("Transaction submission failed: {}", error_message))
            },
        }
    }

    async fn transaction_query(
        &self,
        excess_sig_nonce: Vec<u8>,
        excess_sig_sig: Vec<u8>,
    ) -> Result<TxQueryResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Querying transaction with excess signature nonce {} and signature {}", excess_sig_nonce.to_hex(), excess_sig_sig.to_hex());
        let mut target_url = self.api_address.join("/transactions")?;
        target_url.set_query(Some(&format!(
            "excess_sig_nonce={}&excess_sig_sig={}",
            excess_sig_nonce.to_hex(),
            excess_sig_sig.to_hex()
        )));
        let timer = Instant::now();
        let res = self.http_client.get(target_url).send().await?;
        self.set_last_latency(timer.elapsed());
        if res.status().is_client_error() || res.status().is_server_error() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| "No response body".to_string());
            warn!(target: LOG_TARGET, "Received error response from Base Node wallet service: {}. {}", status, body);
            return Err(anyhow!(
                "Received error response from Base Node wallet service: {}. {}",
                status,
                body
            ));
        }
        info!(target: LOG_TARGET, "Transaction query successful for excess signature nonce {} and signature {}", excess_sig_nonce.to_hex(), excess_sig_sig.to_hex());
        let response = res.json::<TxQueryResponse>().await?;
        Ok(response)
    }

    async fn get_mempool_fee_per_gram_stats(&self, _count: u64) -> Result<FeePerGramStat, anyhow::Error> {
        Ok(FeePerGramStat {
            order: 1,
            min_fee_per_gram: MicroMinotari::from(1),
            avg_fee_per_gram: MicroMinotari::from(1),
            max_fee_per_gram: MicroMinotari::from(1),
        }) // Placeholder implementation
    }
}
