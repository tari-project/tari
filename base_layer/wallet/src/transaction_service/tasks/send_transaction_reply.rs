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

use crate::output_manager_service::TxId;
use log::*;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::SendMessageResponse};
use tari_p2p::tari_message::TariMessageType;

use crate::transaction_service::{
    error::TransactionServiceError,
    storage::models::InboundTransaction,
    tasks::wait_on_dial::wait_on_dial,
};
use std::time::Duration;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::outbound::{OutboundEncryption, OutboundMessageRequester};
use tari_core::proto::transaction as proto;

const LOG_TARGET: &str = "wallet::transaction_service::tasks::send_transaction_reply";

/// A task to resend a transaction reply message if a repeated Send Transaction is received from a Sender
pub async fn send_transaction_reply(
    inbound_transaction: InboundTransaction,
    mut outbound_message_service: OutboundMessageRequester,
    direct_send_timeout: Duration,
) -> Result<bool, TransactionServiceError>
{
    let recipient_reply = inbound_transaction.receiver_protocol.get_signed_data()?.clone();

    let mut store_and_forward_send_result = false;
    let mut direct_send_result = false;

    let tx_id = inbound_transaction.tx_id;
    let proto_message: proto::RecipientSignedMessage = recipient_reply.into();
    match outbound_message_service
        .send_direct(
            inbound_transaction.source_public_key.clone(),
            OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, proto_message.clone()),
        )
        .await
    {
        Ok(result) => match result {
            SendMessageResponse::Queued(send_states) => {
                if wait_on_dial(
                    send_states,
                    tx_id,
                    inbound_transaction.source_public_key.clone(),
                    "Transaction Reply",
                    direct_send_timeout,
                )
                .await
                {
                    direct_send_result = true;
                } else {
                    store_and_forward_send_result = send_transaction_reply_store_and_forward(
                        tx_id,
                        inbound_transaction.source_public_key.clone(),
                        proto_message.clone(),
                        &mut outbound_message_service,
                    )
                    .await?;
                }
            },
            SendMessageResponse::Failed(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Transaction Reply Send Direct for TxID {} failed: {}", tx_id, err
                );
                store_and_forward_send_result = send_transaction_reply_store_and_forward(
                    tx_id,
                    inbound_transaction.source_public_key.clone(),
                    proto_message.clone(),
                    &mut outbound_message_service,
                )
                .await?;
            },
            SendMessageResponse::PendingDiscovery(rx) => {
                store_and_forward_send_result = send_transaction_reply_store_and_forward(
                    tx_id,
                    inbound_transaction.source_public_key.clone(),
                    proto_message.clone(),
                    &mut outbound_message_service,
                )
                .await?;
                // now wait for discovery to complete
                match rx.await {
                    Ok(send_msg_response) => {
                        if let SendMessageResponse::Queued(send_states) = send_msg_response {
                            debug!(
                                target: LOG_TARGET,
                                "Discovery of {} completed for TxID: {}", inbound_transaction.source_public_key, tx_id
                            );
                            direct_send_result = wait_on_dial(
                                send_states,
                                tx_id,
                                inbound_transaction.source_public_key.clone(),
                                "Transaction Reply",
                                direct_send_timeout,
                            )
                            .await;
                        }
                    },
                    Err(e) => {
                        debug!(
                            target: LOG_TARGET,
                            "Error waiting for Discovery while sending message to TxId: {} {:?}", tx_id, e
                        );
                    },
                }
            },
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Direct Transaction Reply Send failed: {:?}", e);
        },
    }
    Ok(direct_send_result || store_and_forward_send_result)
}

async fn send_transaction_reply_store_and_forward(
    tx_id: TxId,
    destination_pubkey: CommsPublicKey,
    msg: proto::RecipientSignedMessage,
    outbound_message_service: &mut OutboundMessageRequester,
) -> Result<bool, TransactionServiceError>
{
    match outbound_message_service
        .closest_broadcast(
            NodeId::from_public_key(&destination_pubkey),
            OutboundEncryption::EncryptFor(Box::new(destination_pubkey.clone())),
            vec![],
            OutboundDomainMessage::new(TariMessageType::ReceiverPartialTransactionReply, msg),
        )
        .await
    {
        Ok(send_states) => {
            info!(
                target: LOG_TARGET,
                "Sending Transaction Reply (TxId: {}) to Neighbours for Store and Forward successful with Message \
                 Tags: {:?}",
                tx_id,
                send_states.to_tags(),
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
