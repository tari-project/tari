// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
pub mod http;

use anyhow::Error;
use serde::{Deserialize, Serialize};
use tari_core::{
    base_node::rpc::models::{
        self,
        BlockHeader,
        GetUtxosDeletedInfoResponse,
        GetUtxosMinedInfoResponse,
        SyncUtxosByBlockResponse,
    },
    mempool::FeePerGramStat,
    transactions::transaction_components::{Transaction, TransactionOutput},
};
use tari_shutdown::ShutdownSignal;
use tokio::sync::mpsc;

use crate::client::models::TxSubmissionResponse;

/// Trait that a base node wallet client must implement.
#[async_trait::async_trait]
pub trait BaseNodeWalletClient: Send + Sync + Clone + 'static {
    async fn get_address(&self) -> String;
    async fn is_online(&self) -> bool;
    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, Error>;

    async fn get_header_by_height(&self, height: u64) -> Result<Option<BlockHeader>, Error>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Error>;

    async fn get_utxos_by_block(&self, header_hash: Vec<u8>) -> Result<models::GetUtxosByBlockResponse, Error>;

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        end_header_hash: Vec<u8>,
        shutdown: ShutdownSignal,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponse, Error>>, Error>;

    async fn get_last_request_latency(&self) -> Option<std::time::Duration>;

    async fn get_utxos_mined_info(&self, hashes: Vec<Vec<u8>>) -> Result<GetUtxosMinedInfoResponse, Error>;

    async fn fetch_utxo(&self, hash: Vec<u8>) -> Result<Option<TransactionOutput>, Error>;

    async fn query_deleted_utxos(
        &self,
        hashes: Vec<Vec<u8>>,
        must_include_header: Vec<u8>,
    ) -> Result<GetUtxosDeletedInfoResponse, Error>;

    async fn submit_transaction(&self, transaction: Transaction) -> Result<TxSubmissionResponse, Error>;

    async fn transaction_query(
        &self,
        excess_sig_nonce: Vec<u8>,
        excess_sig_sig: Vec<u8>,
    ) -> Result<models::TxQueryResponse, Error>;

    async fn get_mempool_fee_per_gram_stats(&self, count: u64) -> Result<FeePerGramStat, Error>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<String>,
    pub id: String,
}

impl<T> JsonRpcResponse<T> {
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
