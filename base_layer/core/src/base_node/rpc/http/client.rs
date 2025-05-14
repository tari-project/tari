use crate::base_node::rpc::http::models::TipInfoResponse;
use crate::base_node::rpc::BaseNodeWalletQueryServiceClient;
use reqwest::Url;
use thiserror::Error;

#[derive(Debug, Error, )]
pub enum Error {
    #[error("Failed to parse http address: {0}")]
    HttpAddressParse(#[from] url::ParseError),
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),
}

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
        self.http_client
            .get(self.api_address.join("/get_tip_info")?).send().await?
            .json().await?
    }
}