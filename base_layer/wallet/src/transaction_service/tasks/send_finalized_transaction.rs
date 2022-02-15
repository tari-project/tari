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

use log::*;
use tari_common_types::transaction::TxId;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageResponse},
};
use tari_core::transactions::{transaction_components::Transaction, transaction_protocol::proto};
use tari_p2p::tari_message::TariMessageType;

use crate::transaction_service::{
    config::TransactionRoutingMechanism,
    error::TransactionServiceError,
    tasks::wait_on_dial::wait_on_dial,
};

const LOG_TARGET: &str = "wallet::transaction_service::tasks::send_finalized_transaction";

pub async fn send_finalized_transaction_message(
    tx_id: TxId,
    transaction: Transaction,
    destination_public_key: CommsPublicKey,
    mut outbound_message_service: OutboundMessageRequester,
    direct_send_timeout: Duration,
    transaction_routing_mechanism: TransactionRoutingMechanism,
) -> Result<(), TransactionServiceError> {
    match transaction_routing_mechanism {
        TransactionRoutingMechanism::DirectOnly | TransactionRoutingMechanism::DirectAndStoreAndForward => {
            send_finalized_transaction_message_direct(
                tx_id,
                transaction,
                destination_public_key,
                outbound_message_service,
                direct_send_timeout,
                transaction_routing_mechanism,
            )
            .await?;
        },
        TransactionRoutingMechanism::StoreAndForwardOnly => {
            let finalized_transaction_message = proto::TransactionFinalizedMessage {
                tx_id: tx_id.into(),
                transaction: Some(
                    transaction
                        .clone()
                        .try_into()
                        .map_err(TransactionServiceError::InvalidMessageError)?,
                ),
            };
            let store_and_forward_send_result = send_transaction_finalized_message_store_and_forward(
                tx_id,
                destination_public_key,
                finalized_transaction_message.clone(),
                &mut outbound_message_service,
            )
            .await?;
            if !store_and_forward_send_result {
                return Err(TransactionServiceError::OutboundSendFailure);
            }
        },
    };

    Ok(())
}

pub async fn send_finalized_transaction_message_direct(
    tx_id: TxId,
    transaction: Transaction,
    destination_public_key: CommsPublicKey,
    mut outbound_message_service: OutboundMessageRequester,
    direct_send_timeout: Duration,
    transaction_routing_mechanism: TransactionRoutingMechanism,
) -> Result<(), TransactionServiceError> {
    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: tx_id.into(),
        transaction: Some(
            transaction
                .clone()
                .try_into()
                .map_err(TransactionServiceError::InvalidMessageError)?,
        ),
    };
    let mut store_and_forward_send_result = false;
    let mut direct_send_result = false;
    match outbound_message_service
        .send_direct(
            destination_public_key.clone(),
            OutboundDomainMessage::new(
                TariMessageType::TransactionFinalized,
                finalized_transaction_message.clone(),
            ),
        )
        .await
    {
        Ok(result) => match result {
            SendMessageResponse::Queued(send_states) => {
                if wait_on_dial(
                    send_states,
                    tx_id,
                    destination_public_key.clone(),
                    "Finalized Transaction",
                    direct_send_timeout,
                )
                .await
                {
                    direct_send_result = true;
                }
                // Send a Store and Forward (SAF) regardless.
                info!(
                    target: LOG_TARGET,
                    "Direct Send finalize result was {}. Sending SAF for TxId: {} to recipient with Public Key: {}",
                    direct_send_result,
                    tx_id,
                    destination_public_key,
                );
                if transaction_routing_mechanism == TransactionRoutingMechanism::DirectAndStoreAndForward {
                    store_and_forward_send_result = send_transaction_finalized_message_store_and_forward(
                        tx_id,
                        destination_public_key,
                        finalized_transaction_message.clone(),
                        &mut outbound_message_service,
                    )
                    .await?;
                }
            },
            SendMessageResponse::Failed(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Finalized Transaction Send Direct for TxID {} failed: {}", tx_id, err
                );
                if transaction_routing_mechanism == TransactionRoutingMechanism::DirectAndStoreAndForward {
                    store_and_forward_send_result = send_transaction_finalized_message_store_and_forward(
                        tx_id,
                        destination_public_key.clone(),
                        finalized_transaction_message.clone(),
                        &mut outbound_message_service,
                    )
                    .await?;
                }
            },
            SendMessageResponse::PendingDiscovery(rx) => {
                if transaction_routing_mechanism == TransactionRoutingMechanism::DirectAndStoreAndForward {
                    store_and_forward_send_result = send_transaction_finalized_message_store_and_forward(
                        tx_id,
                        destination_public_key.clone(),
                        finalized_transaction_message.clone(),
                        &mut outbound_message_service,
                    )
                    .await?;
                }
                // now wait for discovery to complete
                match rx.await {
                    Ok(SendMessageResponse::Queued(send_states)) => {
                        debug!(
                            target: LOG_TARGET,
                            "Discovery of {} completed for TxID: {}", destination_public_key, tx_id
                        );
                        direct_send_result = wait_on_dial(
                            send_states,
                            tx_id,
                            destination_public_key.clone(),
                            "Finalized Transaction",
                            direct_send_timeout,
                        )
                        .await;
                    },

                    Ok(SendMessageResponse::Failed(e)) => warn!(
                        target: LOG_TARGET,
                        "Failed to send message ({}) Discovery failed for TxId: {}", e, tx_id
                    ),
                    Ok(SendMessageResponse::PendingDiscovery(_)) => unreachable!(),
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Error waiting for Discovery while sending message to TxId: {} {:?}", tx_id, e
                        );
                    },
                }
            },
        },
        Err(e) => {
            warn!(target: LOG_TARGET, "Direct Finalized Transaction Send failed: {:?}", e);
        },
    }
    if !direct_send_result && !store_and_forward_send_result {
        return Err(TransactionServiceError::OutboundSendFailure);
    }
    Ok(())
}

async fn send_transaction_finalized_message_store_and_forward(
    tx_id: TxId,
    destination_pubkey: CommsPublicKey,
    msg: proto::TransactionFinalizedMessage,
    outbound_message_service: &mut OutboundMessageRequester,
) -> Result<bool, TransactionServiceError> {
    match outbound_message_service
        .closest_broadcast(
            NodeId::from_public_key(&destination_pubkey),
            OutboundEncryption::encrypt_for(destination_pubkey.clone()),
            vec![],
            OutboundDomainMessage::new(TariMessageType::TransactionFinalized, msg.clone()),
        )
        .await
    {
        Ok(send_states) => {
            info!(
                target: LOG_TARGET,
                "Sending Finalized Transaction (TxId: {}) to Neighbours for Store and Forward successful with Message \
                 Tags: {:?}",
                tx_id,
                send_states.to_tags(),
            );
        },
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Sending Finalized Transaction (TxId: {}) to neighbours for Store and Forward failed: {:?}", tx_id, e
            );
            return Ok(false);
        },
    };

    Ok(true)
}
