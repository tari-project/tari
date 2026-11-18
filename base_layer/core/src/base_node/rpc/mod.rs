// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod service;

pub mod sync_utxos_by_block_task;

pub use service::BaseNodeWalletRpcService;

pub mod query_service;

use std::error::Error;

use tari_common_types::types::CompressedCommitment;
use tari_comms::protocol::rpc::{Request, Response, RpcStatus, Streaming};
use tari_comms_rpc_macros::tari_rpc;
pub use tari_transaction_components::rpc::MAX_ALLOWED_QUERY_SIZE;
use tari_transaction_components::{
    rpc::{models, models::GenerateBurnOutputProofResponse},
    transaction_components::TransactionOutput,
};
use url::Url;

use crate::{
    base_node::StateMachineHandle,
    chain_storage::{BlockchainBackend, ChainStorageError, async_db::AsyncBlockchainDb},
    mempool::service::MempoolHandle,
    proto::{
        self,
        base_node::{
            FetchMatchingUtxos,
            FetchUtxosResponse,
            GetMempoolFeePerGramStatsRequest,
            GetMempoolFeePerGramStatsResponse,
            GetWalletQueryHttpServiceAddressResponse,
            QueryDeletedRequest,
            QueryDeletedResponse,
            Signatures,
            SyncUtxosByBlockRequest,
            SyncUtxosByBlockResponse,
            TipInfoResponse,
            TxQueryBatchResponses,
            TxQueryResponse,
            TxSubmissionResponse,
            UtxoQueryRequest,
            UtxoQueryResponses,
        },
        types::{Signature, Transaction},
    },
};

/// Error from [height_at_time]: a header missing during the search is distinct from a failing database
#[derive(Debug, thiserror::Error)]
pub(crate) enum HeightAtTimeError {
    #[error(transparent)]
    Storage(#[from] ChainStorageError),
    #[error("Header not found during search at height {0}")]
    HeaderNotFound(u64),
}

/// Returns the highest height `h` in `[0, tip]` with `timestamp(h) <= epoch_time`: 0 when `epoch_time` is before block
/// 1, and the tip when `epoch_time` is at or after the tip.
///
/// Block timestamps are not strictly monotonic (median-time-past rule), so near non-monotonic stretches the result is
/// approximate. The search still terminates and returns a height in `[0, tip]`.
pub(crate) async fn height_at_time<B: BlockchainBackend + 'static>(
    db: &AsyncBlockchainDb<B>,
    epoch_time: u64,
) -> Result<u64, HeightAtTimeError> {
    let tip = db.fetch_tip_header().await?;
    if tip.timestamp() <= epoch_time {
        return Ok(tip.height());
    }
    // Partition point of `timestamp(h) <= epoch_time` over `[1, tip)`; the tip is already known to be after it
    let (mut low, mut high) = (1u64, tip.height());
    while low < high {
        let mid = low.saturating_add(high.saturating_sub(low) / 2);
        let header = db
            .fetch_header(mid)
            .await?
            .ok_or(HeightAtTimeError::HeaderNotFound(mid))?;
        if header.timestamp.as_u64() <= epoch_time {
            low = mid.saturating_add(1);
        } else {
            high = mid;
        }
    }
    Ok(low.saturating_sub(1))
}

/// Trait that a base node wallet query service must implement.
/// Please note that this service is to fetch data, so read-only queries.
#[async_trait::async_trait]
pub trait BaseNodeWalletQueryService: Send + Sync + 'static {
    type Error: Error + 'static;

    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, Self::Error>;

    async fn get_header_by_height(&self, height: u64) -> Result<models::BlockHeader, Self::Error>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Self::Error>;

    async fn get_utxos_mined_info(
        &self,
        request: models::GetUtxosMinedInfoRequest,
    ) -> Result<models::GetUtxosMinedInfoResponse, Self::Error>;

    async fn get_utxos_by_block(
        &self,
        request: models::GetUtxosByBlockRequest,
    ) -> Result<models::GetUtxosByBlockResponse, Self::Error>;

    async fn transaction_query(&self, signature: models::Signature) -> Result<models::TxQueryResponse, Self::Error>;

    async fn sync_utxos_by_block_v0(
        &self,
        request: models::SyncUtxosByBlockRequest,
    ) -> Result<models::SyncUtxosByBlockResponseV0, Self::Error>;

    async fn sync_utxos_by_block_v1(
        &self,
        request: models::SyncUtxosByBlockRequest,
    ) -> Result<models::SyncUtxosByBlockResponseV1, Self::Error>;

    async fn get_utxos_deleted_info(
        &self,
        request: models::GetUtxosDeletedInfoRequest,
    ) -> Result<models::GetUtxosDeletedInfoResponse, Self::Error>;

    async fn get_utxos_deleted_info_v1(
        &self,
        request: models::GetUtxosDeletedInfoRequest,
    ) -> Result<models::GetUtxosDeletedInfoResponseV1, Self::Error>;

    async fn generate_burn_output_proof(
        &self,
        commitment: CompressedCommitment,
    ) -> Result<GenerateBurnOutputProofResponse, Self::Error>;

    async fn get_utxo(&self, request: models::GetUtxoRequest) -> Result<Option<TransactionOutput>, Self::Error>;

    async fn get_mempool_fee_per_gram_stats(&self, count: usize) -> Result<Vec<models::FeePerGramStat>, Self::Error>;
}

// `max_items` is the decode budget for a method: the most embedded message instances (plus repeated bytes/string
// elements and packed scalar bytes) its request, response or each stream item may carry (see
// `tari_comms::decode_budget`). BODY_MAX_DECODE_ITEMS (262,144) covers a max-weight block body or transaction of fully
// populated (hydrated) inputs plus 1,000 coinbases: ~160k instances on mainnet (~1.6x headroom), ~222k on the
// 127,795-weight networks (~1.2x); see `proto::decode_budget_tests::per_network`. sync_blocks serves compact inputs (2
// instances each), so a synced body has ~7x (mainnet) / ~5.7x headroom; hydrated inputs only reach submit_transaction,
// which rejects an over-budget request without a ban. 65_536 covers the batched queries and streams. The default is
// 16_384. `max_request_items` bounds the request on its own where the handler accepts far less than the response may
// carry: about twice the handler's own count limit (512 hashes or signatures; 1,000 chain-split hashes).
#[tari_rpc(protocol_name = b"t/bnwallet/1", server_struct = BaseNodeWalletRpcServer, client_struct = BaseNodeWalletRpcClient)]
pub trait BaseNodeWalletService: Send + Sync + 'static {
    #[rpc(method = 1, max_items = crate::proto::BODY_MAX_DECODE_ITEMS)]
    async fn submit_transaction(
        &self,
        request: Request<Transaction>,
    ) -> Result<Response<TxSubmissionResponse>, RpcStatus>;

    #[rpc(method = 2)]
    async fn transaction_query(&self, request: Request<Signature>) -> Result<Response<TxQueryResponse>, RpcStatus>;

    #[rpc(method = 3, max_items = 65_536, max_request_items = 1_024)]
    async fn transaction_batch_query(
        &self,
        request: Request<Signatures>,
    ) -> Result<Response<TxQueryBatchResponses>, RpcStatus>;

    #[rpc(method = 4, max_items = 65_536, max_request_items = 1_024)]
    async fn fetch_matching_utxos(
        &self,
        request: Request<FetchMatchingUtxos>,
    ) -> Result<Response<FetchUtxosResponse>, RpcStatus>;

    #[rpc(method = 5)]
    async fn get_tip_info(&self, request: Request<()>) -> Result<Response<TipInfoResponse>, RpcStatus>;

    #[rpc(method = 6)]
    async fn get_header(&self, request: Request<u64>) -> Result<Response<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 7, max_items = 65_536, max_request_items = 1_024)]
    async fn utxo_query(&self, request: Request<UtxoQueryRequest>) -> Result<Response<UtxoQueryResponses>, RpcStatus>;

    #[rpc(method = 8, max_items = 65_536, max_request_items = 1_024)]
    async fn query_deleted(
        &self,
        request: Request<QueryDeletedRequest>,
    ) -> Result<Response<QueryDeletedResponse>, RpcStatus>;

    #[rpc(method = 9)]
    async fn get_header_by_height(
        &self,
        request: Request<u64>,
    ) -> Result<Response<proto::core::BlockHeader>, RpcStatus>;

    #[rpc(method = 10)]
    async fn get_height_at_time(&self, request: Request<u64>) -> Result<Response<u64>, RpcStatus>;

    #[rpc(method = 11, max_items = 65_536)]
    async fn sync_utxos_by_block(
        &self,
        request: Request<SyncUtxosByBlockRequest>,
    ) -> Result<Streaming<SyncUtxosByBlockResponse>, RpcStatus>;

    #[rpc(method = 12)]
    async fn get_mempool_fee_per_gram_stats(
        &self,
        request: Request<GetMempoolFeePerGramStatsRequest>,
    ) -> Result<Response<GetMempoolFeePerGramStatsResponse>, RpcStatus>;

    #[rpc(method = 13)]
    async fn get_wallet_query_http_service_address(
        &self,
        request: Request<()>,
    ) -> Result<Response<GetWalletQueryHttpServiceAddressResponse>, RpcStatus>;
}

pub fn create_base_node_wallet_rpc_service<B: BlockchainBackend + 'static>(
    db: AsyncBlockchainDb<B>,
    mempool: MempoolHandle,
    state_machine: StateMachineHandle,
    wallet_query_service_address: Option<Url>,
) -> BaseNodeWalletRpcServer<BaseNodeWalletRpcService<B>> {
    BaseNodeWalletRpcServer::new(BaseNodeWalletRpcService::new(
        db,
        mempool,
        state_machine,
        wallet_query_service_address,
    ))
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
    use tari_common::configuration::Network;
    use tari_transaction_components::tari_proof_of_work::Difficulty;

    use super::*;
    use crate::{
        consensus::BaseNodeConsensusManager,
        test_helpers::blockchain::{TempDatabase, TestBlockchain},
    };

    const NAMES: [&str; 8] = ["1", "2", "3", "4", "5", "6", "7", "8"];

    fn blockchain() -> TestBlockchain {
        TestBlockchain::create(BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap())
    }

    /// Appends one block per name to the tip, 120s apart
    fn append(blockchain: &mut TestBlockchain, names: &[&'static str]) {
        for name in names {
            blockchain.append_to_tip(block_spec!(*name)).unwrap();
        }
    }

    /// The async db and every header's timestamp, by height
    fn timestamps(blockchain: &TestBlockchain) -> (AsyncBlockchainDb<TempDatabase>, Vec<u64>) {
        let db = blockchain.db();
        let tip = db.get_height().unwrap();
        let ts = (0..=tip)
            .map(|h| db.fetch_header(h).unwrap().unwrap().timestamp.as_u64())
            .collect();
        (db.clone().into(), ts)
    }

    #[tokio::test]
    async fn tip_zero_always_gives_zero() {
        let (db, ts) = timestamps(&blockchain());
        for t in [0, ts[0] - 1, ts[0], ts[0] + 1, u64::MAX] {
            assert_eq!(height_at_time(&db, t).await.unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn tip_one_checks_block_one() {
        let mut blockchain = blockchain();
        append(&mut blockchain, &NAMES[..1]);
        let (db, ts) = timestamps(&blockchain);
        assert_eq!(height_at_time(&db, ts[0]).await.unwrap(), 0);
        assert_eq!(height_at_time(&db, ts[1] - 1).await.unwrap(), 0);
        assert_eq!(height_at_time(&db, ts[1]).await.unwrap(), 1);
        assert_eq!(height_at_time(&db, ts[1] + 1).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn longer_chain_gives_the_highest_height_at_or_before_the_time() {
        let mut blockchain = blockchain();
        append(&mut blockchain, &NAMES);
        let (db, ts) = timestamps(&blockchain);
        let tip = ts.len() as u64 - 1;
        assert_eq!(height_at_time(&db, 0).await.unwrap(), 0);
        assert_eq!(height_at_time(&db, ts[0] - 1).await.unwrap(), 0);
        for (h, t) in ts.iter().enumerate() {
            assert_eq!(height_at_time(&db, *t).await.unwrap(), h as u64);
            assert_eq!(height_at_time(&db, t + 1).await.unwrap(), h as u64);
        }
        assert_eq!(height_at_time(&db, u64::MAX).await.unwrap(), tip);
    }

    #[tokio::test]
    async fn non_monotonic_timestamps_stay_in_range() {
        let mut blockchain = blockchain();
        append(&mut blockchain, &NAMES[..5]);
        // Block 6 is timestamped before block 5
        let (mut block, _) = blockchain.create_unmined_block(block_spec!("6->5"));
        block.header.timestamp = block.header.timestamp.checked_sub(180.into()).unwrap();
        let block = blockchain.mine_block("5", block, Difficulty::min());
        blockchain.append_block("6", block).unwrap();
        append(&mut blockchain, &NAMES[6..]);
        let (db, ts) = timestamps(&blockchain);
        assert!(ts[5] > ts[6]);

        let tip = ts.len() as u64 - 1;
        for t in ts[0] - 1..=ts[ts.len() - 1] + 1 {
            assert!(height_at_time(&db, t).await.unwrap() <= tip);
        }
        assert_eq!(height_at_time(&db, ts[3]).await.unwrap(), 3);
        assert_eq!(height_at_time(&db, ts[ts.len() - 1]).await.unwrap(), tip);
    }
}
