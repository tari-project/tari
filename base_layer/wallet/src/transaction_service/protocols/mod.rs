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

use crate::transaction_service::error::{TransactionServiceError, TransactionServiceProtocolError};

pub mod check_faux_transaction_status;
pub mod fetch_claim_burn_merkle_proofs;
pub mod transaction_broadcast_protocol;
pub mod transaction_validation_protocol;

const LOG_TARGET: &str = "wallet::transaction_service::protocols";

/// The largest serialised transaction the wallet will negotiate and broadcast. Space is reserved for the frame
/// overhead and for coinbases, keeping a single transaction just under 4 MiB. This was historically derived as
/// `6 MiB RPC frame - 2 MiB - 10 KiB`; it is fixed here so that raising the RPC frame size does not change it.
pub const MAX_BROADCAST_TRANSACTION_SIZE: usize = 4 * 1024 * 1024 - 10 * 1024;

/// Verify that the negotiated transaction is not too large to be broadcast
pub fn check_transaction_size<T: Serialize>(
    transaction: &T,
    tx_id: TxId,
) -> Result<(), TransactionServiceProtocolError<TxId>> {
    let mut buf: Vec<u8> = Vec::new();
    serialize_into(&mut buf, transaction).map_err(|e| {
        TransactionServiceProtocolError::new(tx_id, TransactionServiceError::SerializationError(e.to_string()))
    })?;
    if buf.len() > MAX_BROADCAST_TRANSACTION_SIZE {
        let err = TransactionServiceProtocolError::new(tx_id, TransactionServiceError::TransactionTooLarge {
            got: buf.len(),
            expected: MAX_BROADCAST_TRANSACTION_SIZE,
        });
        error!(
            target: LOG_TARGET,
            "Transaction '{tx_id}' too large, cannot be broadcast ({err:?})."
        );
        Err(err)
    } else {
        debug!(
            target: LOG_TARGET,
            "Transaction '{}' size ok, can be broadcast (got: {}, limit: {}).",
            tx_id, buf.len(), MAX_BROADCAST_TRANSACTION_SIZE
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_comms::protocol::rpc::RPC_MAX_REQUEST_SIZE;

    use super::MAX_BROADCAST_TRANSACTION_SIZE;

    #[test]
    fn max_broadcast_transaction_size_is_unchanged_and_fits_in_a_request() {
        // The historical value, derived from the old 6 MiB RPC frame
        assert_eq!(
            MAX_BROADCAST_TRANSACTION_SIZE,
            6 * 1024 * 1024 - (2 * 1024 * 1024 + 10 * 1024)
        );
        // A negotiated transaction is submitted to base nodes in an RPC request
        const { assert!(MAX_BROADCAST_TRANSACTION_SIZE < RPC_MAX_REQUEST_SIZE) };
    }
}
