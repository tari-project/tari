// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
use async_trait::async_trait;
use tari_core::{base_node::rpc::models::TipInfoResponse, blocks::BlockHeader};
use url::Url;

use crate::{error::ClientError, BaseNodeWalletClient};

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
}
