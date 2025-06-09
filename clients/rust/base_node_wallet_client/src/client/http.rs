// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use anyhow::{anyhow, Error};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use reqwest::StatusCode;
use tari_core::base_node::{
    rpc::models::{self, BlockHeader, SyncUtxosByBlockResponse, TipInfoResponse},
    state_machine_service::states::Shutdown,
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::hex::Hex;
use tokio::sync::mpsc;
use url::Url;

use crate::BaseNodeWalletClient;

const LOG_TARGET: &str = "tari::wallet::client::http";

/// HTTP client for the Base Node wallet service.
#[derive(Clone)]
pub struct Client {
    api_address: Url,
    http_client: reqwest::Client,
}

impl Client {
    pub fn new(api_address: Url) -> Self {
        Self {
            api_address,
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl BaseNodeWalletClient for Client {
    async fn get_tip_info(&self) -> Result<TipInfoResponse, anyhow::Error> {
        debug!(target: LOG_TARGET, "Requesting tip info from Base Node wallet service at {}", self.api_address);
        let res = self
            .http_client
            .get(self.api_address.join("/get_tip_info")?)
            .send()
            .await?;

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
        let res = self.http_client.get(target_url).send().await?;
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
        let res = self.http_client.get(target_url).send().await?;
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
        let res = self
            .http_client
            .get(target_url)
            .json(&models::GetUtxosByBlockRequest { header_hash })
            .send()
            .await?;
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
                        },
                    },
                    Err(error) => {
                        if let Err(send_error) = resp_tx.send(Err(error.into())).await {
                            error!(target: LOG_TARGET, "Error sending error result: {:?}", send_error);
                        }
                    },
                }

                if has_next_page {
                    page += 1;
                }
            }
        });

        Ok(resp_rx)
    }
}
