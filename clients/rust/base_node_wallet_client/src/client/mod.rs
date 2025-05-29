// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
pub mod http;

use tari_core::{
    base_node::rpc::{models, models::SyncUtxosByBlockResponse},
    blocks::BlockHeader,
};
use tokio::sync::mpsc;

use crate::error::ClientError;

/// Trait that a base node wallet client must implement.
#[async_trait::async_trait]
pub trait BaseNodeWalletClient: Send + Sync + Clone + 'static {
    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, ClientError>;

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, ClientError>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, ClientError>;

    async fn sync_utxos_by_block(
        &self,
        start_header_hash: Vec<u8>,
        end_header_hash: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<SyncUtxosByBlockResponse, ClientError>>, ClientError>;
}
