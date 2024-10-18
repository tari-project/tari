// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

use std::{convert::TryInto, time::Duration};

use tari_common_types::transaction::TxId;
use tari_core::transactions::transaction_components::Transaction;
use tari_network::{identity::PeerId, OutboundMessager, OutboundMessaging};
use tari_p2p::{message::TariNodeMessageSpec, proto};

use crate::transaction_service::{config::TransactionRoutingMechanism, error::TransactionServiceError};

pub async fn send_finalized_transaction_message(
    tx_id: TxId,
    transaction: Transaction,
    peer_id: PeerId,
    mut outbound_message_service: OutboundMessaging<TariNodeMessageSpec>,
    _direct_send_timeout: Duration,
    _transaction_routing_mechanism: TransactionRoutingMechanism,
) -> Result<(), TransactionServiceError> {
    let finalized_transaction_message = proto::transaction_protocol::TransactionFinalizedMessage {
        tx_id: tx_id.into(),
        transaction: Some(
            transaction
                .try_into()
                .map_err(TransactionServiceError::InvalidMessageError)?,
        ),
    };
    outbound_message_service
        .send_message(peer_id, finalized_transaction_message)
        .await?;
    Ok(())
}
