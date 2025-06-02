// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use async_trait::async_trait;
use log::error;
use tari_core::base_node::rpc::models::{BlockHeader, SyncUtxosByBlockResponse, TipInfoResponse};
use tari_utilities::hex::Hex;
use tokio::sync::mpsc;
use url::Url;

use crate::{error::ClientError, BaseNodeWalletClient};

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
    async fn get_tip_info(&self) -> Result<TipInfoResponse, ClientError> {
        Ok(self
            .http_client
            .get(self.api_address.join("/get_tip_info")?)
            .send()
            .await?
            .json::<TipInfoResponse>()
            .await?)
    }

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, ClientError> {
        let mut target_url = self.api_address.join("/get_header_by_height")?;
        target_url.set_query(Some(format!("height={}", height).as_str()));
        Ok(self
            .http_client
            .get(target_url)
            .send()
            .await?
            .json::<BlockHeader>()
            .await?)
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, ClientError> {
        let mut target_url = self.api_address.join("/get_height_at_time")?;
        target_url.set_query(Some(format!("time={}", epoch_time).as_str()));
        Ok(self.http_client.get(target_url).send().await?.json::<u64>().await?)
    }

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        end_header_hash: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponse, ClientError>>, ClientError> {
        let mut target_url = self.api_address.join("/sync_utxos_by_block")?;
        let (resp_tx, resp_rx) = mpsc::channel(5);
        let start_header_hash_hex = start_header_hash.to_hex();
        let end_header_hash_hex = end_header_hash.to_hex();
        let client = self.http_client.clone();

        tokio::spawn(async move {
            let mut page = 0;
            let mut has_next_page = true;
            while has_next_page {
                target_url.set_query(Some(
                    format!(
                        "start_header_hash={}&end_header_hash={}&limit=5&page={}",
                        &start_header_hash_hex, &end_header_hash_hex, page
                    )
                    .as_str(),
                ));
                match client.get(target_url.clone()).send().await {
                    Ok(response) => match response.json::<SyncUtxosByBlockResponse>().await {
                        Ok(response) => {
                            has_next_page = response.has_next_page;
                            if let Err(send_error) = resp_tx.send(Ok(response)).await {
                                error!(target: LOG_TARGET, "Error sending utxo response: {:?}", send_error);
                            }
                        },
                        Err(error) => {
                            if let Err(send_error) = resp_tx.send(Err(ClientError::HttpClient(error))).await {
                                error!(target: LOG_TARGET, "Error sending error result: {:?}", send_error);
                            }
                        },
                    },
                    Err(error) => {
                        if let Err(send_error) = resp_tx.send(Err(ClientError::HttpClient(error))).await {
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
