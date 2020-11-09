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

use crate::chain_storage::ChainStorageError;
use futures::channel::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum BlockchainStateServiceError {
    #[error("Request channel unexpectedly disconnected")]
    RequestChannelDisconnected,
    #[error("Sender canceled reply")]
    SenderReplyCanceled,
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Invalid argument `{arg}` in function `{func}`: {message}")]
    InvalidArguments {
        func: &'static str,
        arg: &'static str,
        message: String,
    },
}

impl From<mpsc::SendError> for BlockchainStateServiceError {
    fn from(_: mpsc::SendError) -> Self {
        Self::RequestChannelDisconnected
    }
}

// Assume here that a oneshot::Canceled error can only come from the handle
impl From<oneshot::Canceled> for BlockchainStateServiceError {
    fn from(_: oneshot::Canceled) -> Self {
        Self::SenderReplyCanceled
    }
}
