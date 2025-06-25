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

use futures::channel::mpsc;
use minotari_app_grpc::tari_rpc;
use tokio::sync::watch;
use tonic::{Request, Response, Status};

pub struct ReadinessGrpcServer {
    readiness_service: ReadinessService,
}

pub struct ReadinessService {
    readiness_rx: watch::Receiver<ReadinessStatus>,
}

#[derive(Clone)]
pub enum ReadinessStatus {
    Ready,
    Recovering,
    BuildingContext,
    Migrating,
    NotReady,
    StartingUp,
}

impl From<ReadinessStatus> for i32 {
    fn from(status: ReadinessStatus) -> i32 {
        match status {
            ReadinessStatus::NotReady => 0,
            ReadinessStatus::StartingUp => 1,
            ReadinessStatus::Migrating => 2,
            ReadinessStatus::Recovering => 3,
            ReadinessStatus::BuildingContext => 4,
            ReadinessStatus::Ready => 5,
        }
    }
}

impl ReadinessService {
    pub fn new() -> (Self, watch::Sender<ReadinessStatus>) {
        let (readiness_tx, readiness_rx) = watch::channel(ReadinessStatus::NotReady);
        let sender = readiness_tx.clone();
        (Self { readiness_rx }, sender)
    }

    pub fn get_status(&self) -> ReadinessStatus {
        self.readiness_rx.borrow().clone()
    }
}

impl ReadinessGrpcServer {
    pub fn new() -> (Self, watch::Sender<ReadinessStatus>) {
        let (readiness_service, readiness_tx) = ReadinessService::new();
        (Self { readiness_service }, readiness_tx)
    }

    fn get_not_available_status(&self) -> Status {
        Status::unavailable("Node is not available. Initializing...")
    }
}

#[tonic::async_trait]
impl tari_rpc::base_node_server::BaseNode for ReadinessGrpcServer {
    type FetchMatchingUtxosStream = mpsc::Receiver<Result<tari_rpc::FetchMatchingUtxosResponse, Status>>;
    type GetActiveValidatorNodesStream = mpsc::Receiver<Result<tari_rpc::GetActiveValidatorNodesResponse, Status>>;
    type GetBlocksStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type GetMempoolTransactionsStream = mpsc::Receiver<Result<tari_rpc::GetMempoolTransactionsResponse, Status>>;
    type GetNetworkDifficultyStream = mpsc::Receiver<Result<tari_rpc::NetworkDifficultyResponse, Status>>;
    type GetPeersStream = mpsc::Receiver<Result<tari_rpc::GetPeersResponse, Status>>;
    type GetSideChainUtxosStream = mpsc::Receiver<Result<tari_rpc::GetSideChainUtxosResponse, Status>>;
    type GetTemplateRegistrationsStream = mpsc::Receiver<Result<tari_rpc::GetTemplateRegistrationResponse, Status>>;
    type GetTokensInCirculationStream = mpsc::Receiver<Result<tari_rpc::ValueAtHeightResponse, Status>>;
    type ListHeadersStream = mpsc::Receiver<Result<tari_rpc::BlockHeaderResponse, Status>>;
    type SearchKernelsStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;
    type SearchPaymentReferencesStream = mpsc::Receiver<Result<tari_rpc::PaymentReferenceResponse, Status>>;
    type SearchPaymentReferencesViaOutputHashStream =
        mpsc::Receiver<Result<tari_rpc::PaymentReferenceResponse, Status>>;
    type SearchUtxosStream = mpsc::Receiver<Result<tari_rpc::HistoricalBlock, Status>>;

    async fn get_network_state(
        &self,
        _request: Request<tari_rpc::GetNetworkStateRequest>,
    ) -> Result<Response<tari_rpc::GetNetworkStateResponse>, Status> {
        let status = self.readiness_service.get_status();
        let response = tari_rpc::GetNetworkStateResponse {
            metadata: None,
            initial_sync_achieved: false,
            base_node_state: tari_rpc::BaseNodeState::StartUp.into(),
            failed_checkpoints: false,
            reward: 0,
            sha3x_estimated_hash_rate: 0,
            monero_randomx_estimated_hash_rate: 0,
            tari_randomx_estimated_hash_rate: 0,
            num_connections: 0,
            liveness_results: Vec::new(),
            readiness_status: status.into(),
        };
        Ok(Response::new(response))
    }

    async fn get_network_status(
        &self,
        _: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::NetworkStatusResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_network_difficulty(
        &self,
        _request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<Self::GetNetworkDifficultyStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn list_headers(
        &self,
        _request: Request<tari_rpc::ListHeadersRequest>,
    ) -> Result<Response<Self::ListHeadersStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_header_by_hash(
        &self,
        _request: Request<tari_rpc::GetHeaderByHashRequest>,
    ) -> Result<Response<tari_rpc::BlockHeaderResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_blocks(
        &self,
        _request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetBlocksStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_block_timing(
        &self,
        _request: Request<tari_rpc::HeightRequest>,
    ) -> Result<Response<tari_rpc::BlockTimingResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_constants(
        &self,
        _request: Request<tari_rpc::BlockHeight>,
    ) -> Result<Response<tari_rpc::ConsensusConstants>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_block_size(
        &self,
        _request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_block_fees(
        &self,
        _request: Request<tari_rpc::BlockGroupRequest>,
    ) -> Result<Response<tari_rpc::BlockGroupResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_version(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::StringValue>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn check_for_updates(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SoftwareUpdate>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_tokens_in_circulation(
        &self,
        _request: Request<tari_rpc::GetBlocksRequest>,
    ) -> Result<Response<Self::GetTokensInCirculationStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_new_block_template(
        &self,
        _request: Request<tari_rpc::NewBlockTemplateRequest>,
    ) -> Result<Response<tari_rpc::NewBlockTemplateResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_new_block(
        &self,
        _request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_new_block_with_coinbases(
        &self,
        _request: Request<tari_rpc::GetNewBlockWithCoinbasesRequest>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_new_block_template_with_coinbases(
        &self,
        _request: Request<tari_rpc::GetNewBlockTemplateWithCoinbasesRequest>,
    ) -> Result<Response<tari_rpc::GetNewBlockResult>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_new_block_blob(
        &self,
        _request: Request<tari_rpc::NewBlockTemplate>,
    ) -> Result<Response<tari_rpc::GetNewBlockBlobResult>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn submit_block(
        &self,
        _request: Request<tari_rpc::Block>,
    ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn submit_block_blob(
        &self,
        _request: Request<tari_rpc::BlockBlobRequest>,
    ) -> Result<Response<tari_rpc::SubmitBlockResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn submit_transaction(
        &self,
        _request: Request<tari_rpc::SubmitTransactionRequest>,
    ) -> Result<Response<tari_rpc::SubmitTransactionResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_sync_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncInfoResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_sync_progress(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::SyncProgressResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_tip_info(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::TipInfoResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn search_kernels(
        &self,
        _request: Request<tari_rpc::SearchKernelsRequest>,
    ) -> Result<Response<Self::SearchKernelsStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn search_utxos(
        &self,
        _request: Request<tari_rpc::SearchUtxosRequest>,
    ) -> Result<Response<Self::SearchUtxosStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn fetch_matching_utxos(
        &self,
        _request: Request<tari_rpc::FetchMatchingUtxosRequest>,
    ) -> Result<Response<Self::FetchMatchingUtxosStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_peers(
        &self,
        _request: Request<tari_rpc::GetPeersRequest>,
    ) -> Result<Response<Self::GetPeersStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_mempool_transactions(
        &self,
        _request: Request<tari_rpc::GetMempoolTransactionsRequest>,
    ) -> Result<Response<Self::GetMempoolTransactionsStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn transaction_state(
        &self,
        _request: Request<tari_rpc::TransactionStateRequest>,
    ) -> Result<Response<tari_rpc::TransactionStateResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn identify(&self, _request: Request<tari_rpc::Empty>) -> Result<Response<tari_rpc::NodeIdentity>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn list_connected_peers(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::ListConnectedPeersResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_mempool_stats(
        &self,
        _request: Request<tari_rpc::Empty>,
    ) -> Result<Response<tari_rpc::MempoolStatsResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_active_validator_nodes(
        &self,
        _request: Request<tari_rpc::GetActiveValidatorNodesRequest>,
    ) -> Result<Response<Self::GetActiveValidatorNodesStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_shard_key(
        &self,
        _request: Request<tari_rpc::GetShardKeyRequest>,
    ) -> Result<Response<tari_rpc::GetShardKeyResponse>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_template_registrations(
        &self,
        _request: Request<tari_rpc::GetTemplateRegistrationsRequest>,
    ) -> Result<Response<Self::GetTemplateRegistrationsStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn get_side_chain_utxos(
        &self,
        _request: Request<tari_rpc::GetSideChainUtxosRequest>,
    ) -> Result<Response<Self::GetSideChainUtxosStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn search_payment_references(
        &self,
        _request: Request<tari_rpc::SearchPaymentReferencesRequest>,
    ) -> Result<Response<Self::SearchPaymentReferencesStream>, Status> {
        return Err(self.get_not_available_status());
    }

    async fn search_payment_references_via_output_hash(
        &self,
        _request: Request<tari_rpc::FetchMatchingUtxosRequest>,
    ) -> Result<Response<Self::SearchPaymentReferencesStream>, Status> {
        return Err(self.get_not_available_status());
    }
}
