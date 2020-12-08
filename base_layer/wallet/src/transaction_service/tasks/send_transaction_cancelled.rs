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
use crate::{output_manager_service::TxId, transaction_service::error::TransactionServiceError};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    outbound::{OutboundEncryption, OutboundMessageRequester},
};
use tari_core::proto::transaction as proto;
use tari_p2p::tari_message::TariMessageType;

pub async fn send_transaction_cancelled_message(
    tx_id: TxId,
    destination_public_key: CommsPublicKey,
    mut outbound_message_service: OutboundMessageRequester,
) -> Result<(), TransactionServiceError>
{
    let proto_message = proto::TransactionCancelledMessage { tx_id };

    // Send both direct and SAF we are not going to monitor the progress on these messages for potential resend as
    // they are just courtesy messages
    let _ = outbound_message_service
        .send_direct(
            destination_public_key.clone(),
            OutboundDomainMessage::new(TariMessageType::TransactionCancelled, proto_message.clone()),
        )
        .await?;

    let _ = outbound_message_service
        .closest_broadcast(
            NodeId::from_public_key(&destination_public_key),
            OutboundEncryption::EncryptFor(Box::new(destination_public_key)),
            vec![],
            OutboundDomainMessage::new(TariMessageType::SenderPartialTransaction, proto_message),
        )
        .await?;
    Ok(())
}
