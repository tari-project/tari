use minotari_node_wallet_client::BaseNodeWalletClient;
use url::Url;

pub trait HttpClientFactory: Clone + Send + Sync + 'static {
    type Client: BaseNodeWalletClient;
    fn create_http_client(&self) -> Self::Client;
    fn new(node_url: Url) -> Self;
}
#[derive(Clone)]
pub struct DefaultHttpClientFactory {
    node_url: Url,
}

impl HttpClientFactory for DefaultHttpClientFactory {
    type Client = minotari_node_wallet_client::http::Client;

    fn new(node_url: Url) -> Self {
        Self { node_url }
    }

    fn create_http_client(&self) -> Self::Client {
        minotari_node_wallet_client::http::Client::new(self.node_url.clone())
    }
}
