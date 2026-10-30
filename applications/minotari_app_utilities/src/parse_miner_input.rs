// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::str::FromStr;

use dialoguer::{Input as InputPrompt, console::Term};
use log::*;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{NewBlockTemplateRequest, base_node_client::BaseNodeClient, sha_p2_pool_client::ShaP2PoolClient},
};
use tari_common::configuration::{
    Network,
    bootstrap::{ApplicationType, grpc_default_port},
};
use tari_common_types::tari_address::TariAddress;
use thiserror::Error;
use tonic::{
    Code,
    codegen::InterceptedService,
    transport::{Channel, Uri},
};

const LOG_TARGET: &str = "minotari::app_utilities::parse_miner_input";

/// User requested quit from interactive prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("User requested quit")]
pub struct UserQuit;

/// Error parsing input
#[derive(Debug, Error)]
pub enum ParseInputError {
    #[error("Could not convert data:{0}")]
    WalletPaymentAddress(String),
    #[error("Could not convert data:{0}")]
    BaseNodeSocketAddress(String),
    #[error("Non-interactive mode (or missing terminal TTY) requires '{0}' to be configured")]
    NonInteractive(&'static str),
    #[error("Prompt interaction error: {0}")]
    PromptError(#[from] dialoguer::Error),
    #[error(transparent)]
    UserQuit(#[from] UserQuit),
}

pub fn prompt_for_base_node_address(network: Network, non_interactive: bool) -> Result<String, ParseInputError> {
    let default_address = format!(
        "http://127.0.0.1:{}",
        grpc_default_port(ApplicationType::BaseNode, network)
    );
    if non_interactive || !Term::stderr().is_term() {
        info!(
            target: LOG_TARGET,
            "Non-interactive mode or missing terminal: assuming default base node address '{default_address}'"
        );
        return Ok(default_address);
    }
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'base-node-grpc-address' ('quit' or 'exit' to quit) ")
            .default(default_address.clone())
            .interact()?;
        process_quit(&address)?;
        if address == default_address {
            info!(target: LOG_TARGET, "Assumed default base node address '{default_address}'");
        }
        if Uri::from_str(&address).is_err() {
            println!("  Error - base node address '{address}' not valid");
            continue;
        }
        // Remove leading and trailing whitespace
        address = address.trim().to_string();
        return Ok(address);
    }
}

pub fn prompt_for_p2pool_address(non_interactive: bool) -> Result<String, ParseInputError> {
    let default_address = format!("http://127.0.0.1:{}", 18145);
    if non_interactive || !Term::stderr().is_term() {
        info!(
            target: LOG_TARGET,
            "Non-interactive mode or missing terminal: assuming default p2pool address '{default_address}'"
        );
        return Ok(default_address);
    }
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'p2pool-grpc-address' ('quit' or 'exit' to quit) ")
            .default(default_address.clone())
            .interact()?;
        process_quit(&address)?;
        if address == default_address {
            info!(target: LOG_TARGET, "Assumed default p2pool address '{default_address}'");
        }
        if Uri::from_str(&address).is_err() {
            println!("  Error - p2pool address '{address}' not valid");
            continue;
        }
        // Remove leading and trailing whitespace
        address = address.trim().to_string();
        return Ok(address);
    }
}

/// Read wallet_payment_address arg or prompt for input
pub fn wallet_payment_address(
    config_wallet_payment_address: String,
    network: Network,
    non_interactive: bool,
) -> Result<TariAddress, ParseInputError> {
    // Verify config setting
    match TariAddress::from_str(&config_wallet_payment_address) {
        Ok(address) => {
            if address == TariAddress::default() {
                if non_interactive || !Term::stderr().is_term() {
                    return Err(ParseInputError::NonInteractive("wallet_payment_address"));
                }
                println!();
                // Get it on the command line
                loop {
                    let mut address = InputPrompt::<String>::new()
                        .with_prompt("Please enter 'wallet-payment-address' ('quit' or 'exit' to quit) ")
                        .interact()?;
                    process_quit(&address)?;
                    // Remove leading and trailing whitespace
                    address = address.trim().to_string();
                    let wallet_address: Result<TariAddress, String> = address.parse().map_err(|e| format!("{e:?}"));
                    match wallet_address {
                        Ok(val) => {
                            if val.network() == network {
                                return Ok(val);
                            } else {
                                println!(
                                    "  Error - wallet payment address '{address}' does not match miner network \
                                     '{network}'"
                                );
                            }
                        },
                        Err(e) => println!("  Error - wallet payment address '{address}' not valid ({e})"),
                    }
                }
            }
            if address.network() != network {
                return Err(ParseInputError::WalletPaymentAddress(format!(
                    "Wallet payment address '{config_wallet_payment_address}' does not match miner network '{network}'"
                )));
            }
            Ok(address)
        },
        Err(err) => Err(ParseInputError::WalletPaymentAddress(format!(
            "Wallet payment address '{config_wallet_payment_address}' not valid ({err})"
        ))),
    }
}

/// User requested quit
pub fn process_quit(command: &str) -> Result<(), UserQuit> {
    if command.to_uppercase() == "QUIT" || command.to_uppercase() == "EXIT" {
        return Err(UserQuit);
    }
    Ok(())
}

/// Base node gRPC client
pub type BaseNodeGrpcClient = BaseNodeClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

/// SHA P2Pool gRPC client
pub type ShaP2PoolGrpcClient = ShaP2PoolClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

/// Verify that the base node is responding to the mining gRPC requests
pub async fn verify_base_node_grpc_mining_responses(
    node_conn: &mut BaseNodeGrpcClient,
    pow_algo_request: NewBlockTemplateRequest,
) -> Result<(), String> {
    if let Err(e) = node_conn.get_tip_info(minotari_app_grpc::tari_rpc::Empty {}).await {
        match e.code() {
            Code::Unauthenticated => {
                return Err(
                    "gRPC authentication failed for 'get_tip_info'; check the configured grpc_authentication \
                     credentials"
                        .to_string(),
                );
            },
            Code::PermissionDenied => {
                return Err(
                    "Base node denied 'get_tip_info'; allow this gRPC mining method in the base node configuration"
                        .to_string(),
                );
            },
            _ => {},
        }
        log::warn!(target: LOG_TARGET, "Base node 'get_tip_info' returned status {}: {}", e.code(), e.message());
    }

    if let Err(e) = node_conn.get_new_block_template(pow_algo_request).await {
        match e.code() {
            Code::Unauthenticated => {
                return Err(
                    "gRPC authentication failed for 'get_new_block_template'; check the configured \
                     grpc_authentication credentials"
                        .to_string(),
                );
            },
            Code::PermissionDenied => {
                return Err(
                    "Base node denied 'get_new_block_template'; allow this gRPC mining method in the base node \
                     configuration"
                        .to_string(),
                );
            },
            _ => {},
        }
        log::warn!(target: LOG_TARGET, "Base node 'get_new_block_template' returned status {}: {}", e.code(), e.message());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use futures::channel::mpsc;
    use minotari_app_grpc::tari_rpc::{
        self,
        NewBlockTemplateRequest,
        NewBlockTemplateResponse,
        TipInfoResponse,
        base_node_server::{BaseNode, BaseNodeServer},
    };
    use tari_common_types::grpc_authentication::GrpcAuthentication;
    use tonic::{Request, Response, Status};

    use super::*;

    struct MockBaseNode {
        call_log: Arc<Mutex<Vec<String>>>,
        tip_info_status: Arc<Mutex<Option<Status>>>,
        template_status: Arc<Mutex<Option<Status>>>,
    }

    #[tonic::async_trait]
    impl BaseNode for MockBaseNode {
        type FetchMatchingUtxosStream = mpsc::Receiver<Result<tari_rpc::FetchMatchingUtxosResponse, Status>>;
        type GetActiveValidatorNodesStream = mpsc::Receiver<Result<tari_rpc::GetActiveValidatorNodesResponse, Status>>;
        type GetBlocksStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
        type GetMempoolTransactionsStream = mpsc::Receiver<Result<tari_rpc::GetMempoolTransactionsResponse, Status>>;
        type GetNetworkDifficultyStream = mpsc::Receiver<Result<tari_rpc::NetworkDifficultyResponse, Status>>;
        type GetPeersStream = mpsc::Receiver<Result<tari_rpc::GetPeersResponse, Status>>;
        type GetSideChainUtxosStream = mpsc::Receiver<Result<tari_rpc::GetSideChainUtxosResponse, Status>>;
        type GetTokensInCirculationStream = mpsc::Receiver<Result<tari_rpc::ValueAtHeightResponse, Status>>;
        type ListHeadersStream = mpsc::Receiver<Result<tari_rpc::BlockHeaderResponse, Status>>;
        type SearchKernelsStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
        type SearchPaymentReferencesStream = mpsc::Receiver<Result<tari_rpc::PaymentReferenceResponse, Status>>;
        type SearchPaymentReferencesViaOutputHashStream =
            mpsc::Receiver<Result<tari_rpc::PaymentReferenceResponse, Status>>;
        type SearchUtxosStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;

        async fn get_tip_info(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<TipInfoResponse>, Status> {
            self.call_log.lock().unwrap().push("get_tip_info".to_string());
            if let Some(status) = self.tip_info_status.lock().unwrap().take() {
                return Err(status);
            }
            Ok(Response::new(TipInfoResponse::default()))
        }

        async fn get_new_block_template(
            &self,
            _request: Request<tari_rpc::NewBlockTemplateRequest>,
        ) -> Result<Response<NewBlockTemplateResponse>, Status> {
            self.call_log.lock().unwrap().push("get_new_block_template".to_string());
            if let Some(status) = self.template_status.lock().unwrap().take() {
                return Err(status);
            }
            Ok(Response::new(NewBlockTemplateResponse::default()))
        }

        async fn get_new_block(
            &self,
            _request: Request<tari_rpc::NewBlockTemplate>,
        ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
            self.call_log.lock().unwrap().push("get_new_block".to_string());
            Ok(Response::new(tari_rpc::GetNewBlockResult::default()))
        }

        async fn submit_block(
            &self,
            _request: Request<tari_rpc::Block>,
        ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
            self.call_log.lock().unwrap().push("submit_block".to_string());
            Ok(Response::new(tari_rpc::SubmitBlockResponse::default()))
        }

        async fn get_network_state(
            &self,
            _request: Request<tari_rpc::GetNetworkStateRequest>,
        ) -> Result<Response<tari_rpc::GetNetworkStateResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_network_status(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_network_difficulty(
            &self,
            _request: Request<tari_rpc::HeightRequest>,
        ) -> Result<Response<Self::GetNetworkDifficultyStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn list_headers(
            &self,
            _request: Request<tari_rpc::ListHeadersRequest>,
        ) -> Result<Response<Self::ListHeadersStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_header_by_hash(
            &self,
            _request: Request<tari_rpc::GetHeaderByHashRequest>,
        ) -> Result<Response<tari_rpc::BlockHeaderResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_blocks(
            &self,
            _request: Request<tari_rpc::GetBlocksRequest>,
        ) -> Result<Response<Self::GetBlocksStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_block_timing(
            &self,
            _request: Request<tari_rpc::HeightRequest>,
        ) -> Result<Response<tari_rpc::BlockTimingResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_constants(
            &self,
            _request: Request<tari_rpc::BlockHeight>,
        ) -> Result<Response<tari_rpc::ConsensusConstants>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_block_size(
            &self,
            _request: Request<tari_rpc::BlockGroupRequest>,
        ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_block_fees(
            &self,
            _request: Request<tari_rpc::BlockGroupRequest>,
        ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_version(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::BaseNodeGetVersionResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn check_for_updates(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::SoftwareUpdate>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_tokens_in_circulation(
            &self,
            _request: Request<tari_rpc::GetBlocksRequest>,
        ) -> Result<Response<Self::GetTokensInCirculationStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_new_block_with_coinbases(
            &self,
            _request: Request<tari_rpc::GetNewBlockWithCoinbasesRequest>,
        ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_new_block_template_with_coinbases(
            &self,
            _request: Request<tari_rpc::GetNewBlockTemplateWithCoinbasesRequest>,
        ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_new_block_blob(
            &self,
            _request: Request<tari_rpc::NewBlockTemplate>,
        ) -> Result<Response<tari_rpc::GetNewBlockBlobResult>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn submit_block_blob(
            &self,
            _request: Request<tari_rpc::BlockBlobRequest>,
        ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn submit_transaction(
            &self,
            _request: Request<tari_rpc::SubmitTransactionRequest>,
        ) -> Result<Response<tari_rpc::SubmitTransactionResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_sync_info(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::SyncInfoResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_sync_progress(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::SyncProgressResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn search_kernels(
            &self,
            _request: Request<tari_rpc::SearchKernelsRequest>,
        ) -> Result<Response<Self::SearchKernelsStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn search_utxos(
            &self,
            _request: Request<tari_rpc::SearchUtxosRequest>,
        ) -> Result<Response<Self::SearchUtxosStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn fetch_matching_utxos(
            &self,
            _request: Request<tari_rpc::FetchMatchingUtxosRequest>,
        ) -> Result<Response<Self::FetchMatchingUtxosStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_peers(
            &self,
            _request: Request<tari_rpc::GetPeersRequest>,
        ) -> Result<Response<Self::GetPeersStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_mempool_transactions(
            &self,
            _request: Request<tari_rpc::GetMempoolTransactionsRequest>,
        ) -> Result<Response<Self::GetMempoolTransactionsStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn transaction_state(
            &self,
            _request: Request<tari_rpc::TransactionStateRequest>,
        ) -> Result<Response<tari_rpc::TransactionStateResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn identify(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::NodeIdentity>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn list_connected_peers(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::ListConnectedPeersResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_mempool_stats(
            &self,
            _request: Request<tari_rpc::Empty>,
        ) -> Result<Response<tari_rpc::MempoolStatsResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_active_validator_nodes(
            &self,
            _request: Request<tari_rpc::GetActiveValidatorNodesRequest>,
        ) -> Result<Response<Self::GetActiveValidatorNodesStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_shard_key(
            &self,
            _request: Request<tari_rpc::GetShardKeyRequest>,
        ) -> Result<Response<tari_rpc::GetShardKeyResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_side_chain_utxos(
            &self,
            _request: Request<tari_rpc::GetSideChainUtxosRequest>,
        ) -> Result<Response<Self::GetSideChainUtxosStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn search_payment_references(
            &self,
            _request: Request<tari_rpc::SearchPaymentReferencesRequest>,
        ) -> Result<Response<Self::SearchPaymentReferencesStream>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn get_validator_node_changes(
            &self,
            _request: Request<tari_rpc::GetValidatorNodeChangesRequest>,
        ) -> Result<Response<tari_rpc::GetValidatorNodeChangesResponse>, Status> {
            Err(Status::unimplemented("unused"))
        }

        async fn search_payment_references_via_output_hash(
            &self,
            _request: Request<tari_rpc::FetchMatchingUtxosRequest>,
        ) -> Result<Response<Self::SearchPaymentReferencesViaOutputHashStream>, Status> {
            Err(Status::unimplemented("unused"))
        }
    }

    async fn setup_mock_client(mock: MockBaseNode) -> (BaseNodeGrpcClient, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(BaseNodeServer::new(mock))
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener))
                .await
            {
                eprintln!("Mock base node server error: {e}");
            }
        });

        let mut client = None;
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let channel = match tonic::transport::Endpoint::from_shared(format!("http://{addr}")) {
                Ok(endpoint) => endpoint.connect().await.ok(),
                Err(_) => None,
            };
            if let Some(channel) = channel {
                let c = BaseNodeClient::with_interceptor(
                    channel,
                    ClientAuthenticationInterceptor::create(&GrpcAuthentication::None).unwrap(),
                );
                client = Some(c);
                break;
            }
        }
        (
            client.expect("Failed to connect to mock base node server"),
            server_handle,
        )
    }

    #[tokio::test]
    async fn test_verify_mining_responses_fails_on_unauthenticated() {
        let call_log = Arc::new(Mutex::new(Vec::new()));
        let mock = MockBaseNode {
            call_log: call_log.clone(),
            tip_info_status: Arc::new(Mutex::new(Some(Status::unauthenticated("invalid credentials")))),
            template_status: Arc::new(Mutex::new(None)),
        };

        let (mut client, server_handle) = setup_mock_client(mock).await;
        let result = verify_base_node_grpc_mining_responses(&mut client, NewBlockTemplateRequest::default()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("authentication failed"));
        assert!(err.contains("grpc_authentication credentials"));
        assert_eq!(*call_log.lock().unwrap(), vec!["get_tip_info"]);

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_verify_mining_responses_distinguishes_permission_denied() {
        let call_log = Arc::new(Mutex::new(Vec::new()));
        let mock = MockBaseNode {
            call_log: call_log.clone(),
            tip_info_status: Arc::new(Mutex::new(Some(Status::permission_denied("method disabled")))),
            template_status: Arc::new(Mutex::new(None)),
        };

        let (mut client, server_handle) = setup_mock_client(mock).await;
        let result = verify_base_node_grpc_mining_responses(&mut client, NewBlockTemplateRequest::default()).await;
        let err = result.unwrap_err();
        assert!(err.contains("denied 'get_tip_info'"));
        assert!(err.contains("allow this gRPC mining method"));
        assert_eq!(*call_log.lock().unwrap(), vec!["get_tip_info"]);

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_verify_mining_responses_succeeds_with_warning_on_unavailable() {
        let call_log = Arc::new(Mutex::new(Vec::new()));
        let mock = MockBaseNode {
            call_log: call_log.clone(),
            tip_info_status: Arc::new(Mutex::new(Some(Status::unavailable("peer busy")))),
            template_status: Arc::new(Mutex::new(None)),
        };

        let (mut client, server_handle) = setup_mock_client(mock).await;
        let result = verify_base_node_grpc_mining_responses(&mut client, NewBlockTemplateRequest::default()).await;
        assert!(result.is_ok());
        assert_eq!(*call_log.lock().unwrap(), vec![
            "get_tip_info",
            "get_new_block_template"
        ]);

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_verify_mining_responses_makes_no_get_new_block_or_submit_block_calls() {
        let call_log = Arc::new(Mutex::new(Vec::new()));
        let mock = MockBaseNode {
            call_log: call_log.clone(),
            tip_info_status: Arc::new(Mutex::new(None)),
            template_status: Arc::new(Mutex::new(None)),
        };

        let (mut client, server_handle) = setup_mock_client(mock).await;
        let result = verify_base_node_grpc_mining_responses(&mut client, NewBlockTemplateRequest::default()).await;
        assert!(result.is_ok());
        let calls = call_log.lock().unwrap().clone();
        assert!(calls.contains(&"get_tip_info".to_string()));
        assert!(calls.contains(&"get_new_block_template".to_string()));
        assert!(!calls.contains(&"get_new_block".to_string()));
        assert!(!calls.contains(&"submit_block".to_string()));

        server_handle.abort();
    }
}

#[cfg(test)]
mod test {
    use std::process::{Command, Stdio};

    use super::*;

    const VALID_MAINNET_ADDRESS: &str =
        "126J92Yow5y9UoRFd1DNujPmVFq9C1ZeiYWT95UKxz5Y1rzbfjtHg4SCZS1dk83ivzt3m2XRQHTaYUk9SwmyeCvy5BJ";

    #[test]
    fn test_process_quit_returns_user_quit() {
        assert!(matches!(process_quit("quit"), Err(UserQuit)));
        assert!(matches!(process_quit("QUIT"), Err(UserQuit)));
        assert!(matches!(process_quit("exit"), Err(UserQuit)));
        assert!(matches!(process_quit("EXIT"), Err(UserQuit)));
        assert!(process_quit("continue").is_ok());
        assert!(process_quit("").is_ok());
    }

    #[test]
    fn test_wallet_payment_address_non_interactive_error() {
        let default_addr = TariAddress::default().to_string();
        let res = wallet_payment_address(default_addr, Network::Esmeralda, true);
        match res {
            Err(ParseInputError::NonInteractive(key)) => assert_eq!(key, "wallet_payment_address"),
            _ => panic!("Expected NonInteractive error, got {res:?}"),
        }
    }

    #[test]
    fn test_prompt_for_base_node_address_non_interactive_fallback() {
        let res = prompt_for_base_node_address(Network::Esmeralda, true).expect("should fall back to default");
        let expected = format!(
            "http://127.0.0.1:{}",
            grpc_default_port(ApplicationType::BaseNode, Network::Esmeralda)
        );
        assert_eq!(res, expected);
    }

    #[test]
    fn test_prompt_for_p2pool_address_non_interactive_fallback() {
        let res = prompt_for_p2pool_address(true).expect("should fall back to default");
        assert_eq!(res, "http://127.0.0.1:18145");
    }

    #[test]
    fn test_wallet_payment_address_valid_config() {
        let valid_addr = TariAddress::from_str(VALID_MAINNET_ADDRESS).expect("fixture must be a valid TariAddress");
        let res = wallet_payment_address(VALID_MAINNET_ADDRESS.to_string(), valid_addr.network(), true)
            .expect("valid address should succeed");
        assert_eq!(res, valid_addr);
    }

    #[test]
    fn test_wallet_payment_address_network_mismatch() {
        let valid_addr = TariAddress::from_str(VALID_MAINNET_ADDRESS).expect("fixture must be a valid TariAddress");
        let other_net = if valid_addr.network() == Network::MainNet {
            Network::Esmeralda
        } else {
            Network::MainNet
        };
        let res = wallet_payment_address(VALID_MAINNET_ADDRESS.to_string(), other_net, true);
        assert!(matches!(res, Err(ParseInputError::WalletPaymentAddress(_))));
    }

    #[test]
    fn test_prompts_non_tty_with_null_stdin() {
        if std::env::var("TARI_TEST_NON_TTY_CHILD").is_ok() {
            assert!(!Term::stderr().is_term(), "child stderr must not be a TTY");
            let default_addr = TariAddress::default().to_string();
            let err = wallet_payment_address(default_addr, Network::Esmeralda, false)
                .expect_err("default address without TTY must error even when non_interactive is false");
            assert!(matches!(err, ParseInputError::NonInteractive("wallet_payment_address")));
            assert!(err.to_string().contains("wallet_payment_address"));
            assert!(err.to_string().contains("TTY"));

            let default_base = format!(
                "http://127.0.0.1:{}",
                grpc_default_port(ApplicationType::BaseNode, Network::Esmeralda)
            );
            let base_res = prompt_for_base_node_address(Network::Esmeralda, false)
                .expect("base node prompt without TTY must fall back to default");
            assert_eq!(base_res, default_base);

            let p2pool_res =
                prompt_for_p2pool_address(false).expect("p2pool prompt without TTY must fall back to default");
            assert_eq!(p2pool_res, "http://127.0.0.1:18145");
            return;
        }

        let exe = std::env::current_exe().expect("current test executable path");
        let status = Command::new(exe)
            .arg("--exact")
            .arg("parse_miner_input::test::test_prompts_non_tty_with_null_stdin")
            .env("TARI_TEST_NON_TTY_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("failed to spawn child test process");
        assert!(status.success(), "non-TTY child test failed with status: {status}");
    }
}
