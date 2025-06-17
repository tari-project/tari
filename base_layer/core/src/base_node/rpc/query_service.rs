// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::trace;
use thiserror::Error;

use crate::{
    base_node::{
        rpc::{models::TipInfoResponse, BaseNodeWalletQueryService},
        state_machine_service::states::StateInfo,
        StateMachineHandle,
    },
    blocks::BlockHeader,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError},
};

const LOG_TARGET: &str = "c::bn::rpc::query_service";

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to get chain metadata: {0}")]
    FailedToGetChainMetadata(#[from] ChainStorageError),
    #[error("Header not found at height: {height}")]
    HeaderNotFound { height: u64 },
}

pub struct Service<B> {
    db: AsyncBlockchainDb<B>,
    state_machine: StateMachineHandle,
}

impl<B: BlockchainBackend + 'static> Service<B> {
    pub fn new(db: AsyncBlockchainDb<B>, state_machine: StateMachineHandle) -> Self {
        Self { db, state_machine }
    }

    fn state_machine(&self) -> StateMachineHandle {
        self.state_machine.clone()
    }
}

#[async_trait::async_trait]
impl<B: BlockchainBackend + 'static> BaseNodeWalletQueryService for Service<B> {
    type Error = Error;

    async fn get_tip_info(&self) -> Result<TipInfoResponse, Self::Error> {
        let state_machine = self.state_machine();
        let status_watch = state_machine.get_status_info_watch();
        let is_synced = match status_watch.borrow().state_info {
            StateInfo::Listening(li) => li.is_synced(),
            _ => false,
        };

        let metadata = self.db.get_chain_metadata().await?;

        Ok(TipInfoResponse {
            metadata: Some(metadata),
            is_synced,
        })
    }

    async fn get_header_by_height(&self, height: u64) -> Result<BlockHeader, Self::Error> {
        Ok(self
            .db
            .fetch_header(height)
            .await?
            .ok_or(Error::HeaderNotFound { height })?)
    }

    async fn get_height_at_time(&self, epoch_time: u64) -> Result<u64, Self::Error> {
        trace!(target: LOG_TARGET, "requested_epoch_time: {}", epoch_time);
        let tip_header = self.db.fetch_tip_header().await?;

        let mut left_height = 0u64;
        let mut right_height = tip_header.height();

        while left_height <= right_height {
            let mut mid_height = (left_height + right_height) / 2;

            if mid_height == 0 {
                return Ok(0u64);
            }
            // If the two bounds are adjacent then perform the test between the right and left sides
            if left_height == mid_height {
                mid_height = right_height;
            }

            let mid_header = self
                .db
                .fetch_header(mid_height)
                .await?
                .ok_or_else(|| Error::HeaderNotFound { height: mid_height })?;
            let before_mid_header = self
                .db
                .fetch_header(mid_height - 1)
                .await?
                .ok_or_else(|| Error::HeaderNotFound { height: mid_height - 1 })?;
            trace!(
                target: LOG_TARGET,
                "requested_epoch_time: {}, left: {}, mid: {}/{} ({}/{}), right: {}",
                epoch_time,
                left_height,
                mid_height,
                mid_height-1,
                mid_header.timestamp.as_u64(),
                before_mid_header.timestamp.as_u64(),
                right_height
            );
            if epoch_time < mid_header.timestamp.as_u64() && epoch_time >= before_mid_header.timestamp.as_u64() {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {}, selected height: {}",
                    epoch_time, before_mid_header.height
                );
                return Ok(before_mid_header.height);
            } else if mid_height == right_height {
                trace!(
                    target: LOG_TARGET,
                    "requested_epoch_time: {}, selected height: {}",
                    epoch_time, right_height
                );
                return Ok(right_height);
            } else if epoch_time <= mid_header.timestamp.as_u64() {
                right_height = mid_height;
            } else {
                left_height = mid_height;
            }
        }

        Ok(0u64)
    }
}
