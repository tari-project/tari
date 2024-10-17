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

use std::{convert::TryInto, time::Duration};

use log::*;
use tari_common_types::{transaction::TxId, types::PublicKey};
use tari_network::{OutboundMessager, OutboundMessaging, ToPeerId};
use tari_p2p::{message::TariNodeMessageSpec, proto::transaction_protocol as proto};

use crate::transaction_service::{
    config::TransactionRoutingMechanism,
    error::TransactionServiceError,
    storage::models::InboundTransaction,
};

const LOG_TARGET: &str = "wallet::transaction_service::tasks::send_transaction_reply";

/// A task to resend a transaction reply message if a repeated Send Transaction is received from a Sender
/// either directly, via Store-and-forward or both as per config setting.
pub async fn send_transaction_reply(
    inbound_transaction: InboundTransaction,
    mut outbound_message_service: OutboundMessaging<TariNodeMessageSpec>,
    direct_send_timeout: Duration,
    transaction_routing_mechanism: TransactionRoutingMechanism,
) -> Result<bool, TransactionServiceError> {
    let recipient_reply = inbound_transaction.receiver_protocol.get_signed_data()?.clone();
    let proto_message: proto::RecipientSignedMessage = recipient_reply
        .try_into()
        .map_err(TransactionServiceError::ServiceError)?;

    let send_result = match transaction_routing_mechanism {
        TransactionRoutingMechanism::DirectOnly | TransactionRoutingMechanism::DirectAndStoreAndForward => {
            send_transaction_reply_direct(
                inbound_transaction,
                outbound_message_service,
                direct_send_timeout,
                transaction_routing_mechanism,
            )
            .await?
        },
        TransactionRoutingMechanism::StoreAndForwardOnly => {
            send_transaction_reply_store_and_forward(
                inbound_transaction.tx_id,
                inbound_transaction.source_address.comms_public_key().clone(),
                proto_message.clone(),
                &mut outbound_message_service,
            )
            .await?
        },
    };

    Ok(send_result)
}

/// A task to resend a transaction reply message if a repeated Send Transaction is received from a Sender
pub async fn send_transaction_reply_direct(
    inbound_transaction: InboundTransaction,
    mut outbound_message_service: OutboundMessaging<TariNodeMessageSpec>,
    _direct_send_timeout: Duration,
    _transaction_routing_mechanism: TransactionRoutingMechanism,
) -> Result<bool, TransactionServiceError> {
    let recipient_reply = inbound_transaction.receiver_protocol.get_signed_data()?.clone();

    let mut direct_send_result = false;

    let proto_message: proto::RecipientSignedMessage = recipient_reply
        .try_into()
        .map_err(TransactionServiceError::ServiceError)?;
    match outbound_message_service
        .send_message(
            inbound_transaction.source_address.comms_public_key().to_peer_id(),
            proto_message,
        )
        .await
    {
        Ok(_) => {
            direct_send_result = true;
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Direct Transaction Reply Send failed: {:?}", e);
        },
    }
    Ok(direct_send_result)
}

async fn send_transaction_reply_store_and_forward(
    tx_id: TxId,
    destination_pubkey: PublicKey,
    msg: proto::RecipientSignedMessage,
    outbound_message_service: &mut OutboundMessaging<TariNodeMessageSpec>,
) -> Result<bool, TransactionServiceError> {
    // If I make this a warn, this will occur often in logs
    debug!(target: LOG_TARGET, "Transaction routing mechanism not supported. Sending direct.");
    match outbound_message_service
        .send_message(destination_pubkey.to_peer_id(), msg)
        .await
    {
        Ok(_) => {
            info!(
                target: LOG_TARGET,
                "Sending Transaction Reply (TxId: {}) to Neighbours for Store and Forward successful",
                tx_id,
            );
        },
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Sending Transaction Reply (TxId: {}) to neighbours for Store and Forward failed: {:?}", tx_id, e,
            );
            return Ok(false);
        },
    };

    Ok(true)
}
