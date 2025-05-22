/*
 * Copyright 2025 The Tari Project
 * SPDX-License-Identifier: BSD-3-Clause
 */
use crate::error::ClientError;
use tari_core::base_node::rpc::models;
use tari_core::blocks::BlockHeader;

/// Trait that a base node wallet client must implement.
#[async_trait::async_trait]
pub trait BaseNodeWalletClient: Send + Sync + Clone + 'static {
    async fn get_tip_info(&self) -> Result<models::TipInfoResponse, ClientError>;

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, ClientError>;

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, ClientError>;
}



