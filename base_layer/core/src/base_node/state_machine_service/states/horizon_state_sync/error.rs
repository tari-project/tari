//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    base_node::{comms_interface::CommsInterfaceError, state_machine_service::states::block_sync::BlockSyncError},
    chain_storage::{ChainStorageError, MmrTree},
    transactions::transaction::TransactionError,
    validation::ValidationError,
};
use thiserror::Error;
use tokio::task;

#[derive(Debug, Error)]
pub enum HorizonSyncError {
    #[error("Peer sent an empty response")]
    EmptyResponse,
    #[error("Peer sent an invalid response")]
    IncorrectResponse,
    #[error("Exceeded maximum sync attempts")]
    MaxSyncAttemptsReached,
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Comms interface error: {0}")]
    CommsInterfaceError(#[from] CommsInterfaceError),
    #[error("Block sync error: {0}")]
    BlockSyncError(#[from] BlockSyncError),
    #[error("Final state validation failed: {0}")]
    FinalStateValidationFailed(ValidationError),
    #[error("Join error: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("Invalid kernel signature: {0}")]
    InvalidKernelSignature(TransactionError),
    #[error("Validation failed for {0} MMR")]
    InvalidMmrRoot(MmrTree),
}

impl HorizonSyncError {
    pub fn is_recoverable(&self) -> bool {
        use HorizonSyncError::*;
        match self {
            FinalStateValidationFailed(_) | InvalidMmrRoot(_) => false,
            _ => true,
        }
    }
}
