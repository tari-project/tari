// Copyright 2020, The Tari Project
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

use futures::{Stream, StreamExt};
use std::sync::Arc;
use tari_wallet::transaction_service::handle::TransactionEvent;

/// Asynchronously processes the event stream checking to see if the given tx_id is present or not
/// ## Parameters
/// `event_stream` - The stream of events to search
/// `expected_tx_id` - The transaction id to be searched for
///
/// ## Returns
/// True if found, false otherwise
pub async fn wait_for_discovery_transaction_event<S>(mut event_stream: S, expected_tx_id: u64) -> bool
where S: Stream<Item = Arc<TransactionEvent>> + Unpin {
    loop {
        match event_stream.next().await {
            Some(event) => {
                if let TransactionEvent::TransactionSendDiscoveryComplete(tx_id, is_success) = &*event {
                    if *tx_id == expected_tx_id {
                        break *is_success;
                    }
                }
            },
            None => {
                break false;
            },
        }
    }
}
