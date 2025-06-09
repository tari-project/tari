// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
pub mod http;

use tari_core::base_node::rpc::{
    models,
    models::{BlockHeader, SyncUtxosByBlockResponse},
};
use tari_shutdown::ShutdownSignal;
use tokio::sync::mpsc;
use anyhow::Error;


/// Trait that a base node wallet client must implement.
#[async_trait::async_trait]
pub trait BaseNodeWalletClient: Send + Sync + Clone + 'static {
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
}
