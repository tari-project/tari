use crate::base_node::rpc::BaseNodeWalletQueryServiceClient;
use crate::proto::base_node::TipInfoResponse;
use reqwest::Url;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {}

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

#[async_trait::async_trait]
impl BaseNodeWalletQueryServiceClient for Client {
    type Error = Error;

    async fn get_tip_info(&self) -> Result<TipInfoResponse, Self::Error> {
        todo!()
    }
}