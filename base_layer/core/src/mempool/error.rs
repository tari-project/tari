//  Copyright 2019 The Tari Project
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

use std::time::Duration;

use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::{
    common::BanReason,
    mempool::unconfirmed_pool::UnconfirmedPoolError,
    transactions::transaction_components::TransactionError,
};

#[derive(Debug, Error)]
pub enum MempoolError {
    #[error("Unconfirmed pool error: `{0}`")]
    UnconfirmedPoolError(#[from] UnconfirmedPoolError),
    #[error("Transaction error: `{0}`")]
    TransactionError(#[from] TransactionError),
    #[error("Internal reply channel error: `{0}`")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("The transaction did not contain any kernels")]
    TransactionNoKernels,
    #[error("Mempool lock poisoned. This indicates that the mempool has panicked while holding a RwLockGuard.")]
    RwLockPoisonError,
    #[error(transparent)]
    BlockingTaskError(#[from] JoinError),
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Mempool indexes out of sync: transaction exists in txs_by_signature but not in tx_by_key")]
    IndexOutOfSync,
}
impl MempoolError {
    pub fn get_ban_reason(&self, short_ban: Duration, long_ban: Duration) -> Option<BanReason> {
        match self {
            _err @ MempoolError::UnconfirmedPoolError(e) => e.get_ban_reason(short_ban, long_ban),
            err @ MempoolError::TransactionError(_) | err @ MempoolError::TransactionNoKernels => Some(BanReason {
                reason: err.to_string(),
                ban_duration: long_ban,
            }),
            err @ MempoolError::TransportChannelError(_) => Some(BanReason {
                reason: err.to_string(),
                ban_duration: short_ban,
            }),
            _err @ MempoolError::RwLockPoisonError |
            _err @ MempoolError::BlockingTaskError(_) |
            _err @ MempoolError::InternalError(_) |
            _err @ MempoolError::IndexOutOfSync => None,
        }
    }
}
