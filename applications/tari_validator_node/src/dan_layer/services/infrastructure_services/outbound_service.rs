// Copyright 2021. The Tari Project
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

use crate::{
    dan_layer::{
        models::{HotStuffMessage, Payload},
        services::infrastructure_services::NodeAddressable,
    },
    digital_assets_error::DigitalAssetError,
    p2p::proto,
};
use async_trait::async_trait;

use crate::dan_layer::models::TariDanPayload;
use std::marker::PhantomData;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::{domain_message::OutboundDomainMessage, outbound::OutboundMessageRequester};
use tari_p2p::tari_message::TariMessageType;
use tokio::sync::mpsc::Sender;

#[async_trait]
pub trait OutboundService<TAddr: NodeAddressable + Send, TPayload: Payload> {
    async fn send(
        &mut self,
        from: TAddr,
        to: TAddr,
        message: HotStuffMessage<TPayload>,
    ) -> Result<(), DigitalAssetError>;

    async fn broadcast(
        &mut self,
        from: TAddr,
        committee: &[TAddr],
        message: HotStuffMessage<TPayload>,
    ) -> Result<(), DigitalAssetError>;
}

pub struct TariCommsOutboundService<TPayload: Payload> {
    outbound_message_requester: OutboundMessageRequester,
    loopback_service: Sender<(CommsPublicKey, HotStuffMessage<TPayload>)>,
    // TODO: Remove
    phantom: PhantomData<TPayload>,
}

impl<TPayload: Payload> TariCommsOutboundService<TPayload> {
    pub fn new(
        outbound_message_requester: OutboundMessageRequester,
        loopback_service: Sender<(CommsPublicKey, HotStuffMessage<TPayload>)>,
    ) -> Self {
        Self {
            outbound_message_requester,
            loopback_service,
            phantom: PhantomData,
        }
    }
}

#[async_trait]
impl OutboundService<CommsPublicKey, TariDanPayload> for TariCommsOutboundService<TariDanPayload> {
    async fn send(
        &mut self,
        from: CommsPublicKey,
        to: CommsPublicKey,
        message: HotStuffMessage<TariDanPayload>,
    ) -> Result<(), DigitalAssetError> {
        // Tari comms does allow sending to itself
        if from == to {
            self.loopback_service.send((from, message)).await.unwrap();
            return Ok(());
        }

        let inner = proto::dan::HotStuffMessage::from(message);
        let tari_message = OutboundDomainMessage::new(TariMessageType::DanConsensusMessage, inner);

        self.outbound_message_requester
            .send_direct(to, tari_message)
            .await
            .unwrap();
        Ok(())
    }

    async fn broadcast(
        &mut self,
        from: CommsPublicKey,
        committee: &[CommsPublicKey],
        message: HotStuffMessage<TariDanPayload>,
    ) -> Result<(), DigitalAssetError> {
        for committee_member in committee {
            // TODO: send in parallel
            self.send(from.clone(), committee_member.clone(), message.clone())
                .await?;
        }
        Ok(())
    }
}
