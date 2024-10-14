// Copyright 2020. The Tari Project
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

use bincode::serialize_into;
use log::{debug, error};
use serde::Serialize;
use tari_common_types::transaction::TxId;
use tari_rpc_framework::RPC_MAX_FRAME_SIZE;

use crate::transaction_service::error::{TransactionServiceError, TransactionServiceProtocolError};

pub mod transaction_broadcast_protocol;
pub mod transaction_receive_protocol;
pub mod transaction_send_protocol;
pub mod transaction_validation_protocol;

const LOG_TARGET: &str = "wallet::transaction_service::protocols";

/// Verify that the negotiated transaction is not too large to be broadcast
pub fn check_transaction_size<T: Serialize>(
    transaction: &T,
    tx_id: TxId,
) -> Result<(), TransactionServiceProtocolError<TxId>> {
    let mut buf: Vec<u8> = Vec::new();
    serialize_into(&mut buf, transaction).map_err(|e| {
        TransactionServiceProtocolError::new(tx_id, TransactionServiceError::SerializationError(e.to_string()))
    })?;
    const SIZE_MARGIN: usize = 1024 * 10;
    if buf.len() > RPC_MAX_FRAME_SIZE.saturating_sub(SIZE_MARGIN) {
        let err = TransactionServiceProtocolError::new(tx_id, TransactionServiceError::TransactionTooLarge {
            got: buf.len(),
            expected: RPC_MAX_FRAME_SIZE.saturating_sub(SIZE_MARGIN),
        });
        error!(
            target: LOG_TARGET,
            "Transaction '{}' too large, cannot be broadcast ({:?}).",
            tx_id, err
        );
        Err(err)
    } else {
        debug!(
            target: LOG_TARGET,
            "Transaction '{}' size ok, can be broadcast (got: {}, limit: {}).",
            tx_id, buf.len(), RPC_MAX_FRAME_SIZE.saturating_sub(SIZE_MARGIN)
        );
        Ok(())
    }
}
