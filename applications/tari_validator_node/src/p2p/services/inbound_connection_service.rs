//  Copyright 2021. The Tari Project
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
use std::{convert::TryInto, sync::Arc};

use async_trait::async_trait;
use futures::{self, pin_mut, Stream, StreamExt};
use tari_common_types::types::PublicKey;
use tari_comms::types::CommsPublicKey;
use tari_dan_core::{
    models::{HotStuffMessage, Payload, TariDanPayload},
    services::infrastructure_services::InboundConnectionService,
    DigitalAssetError,
};
use tari_p2p::comms_connector::PeerMessage;
use tari_shutdown::ShutdownSignal;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::p2p::proto;

pub struct TariCommsInboundConnectionService {
    // TODO: remove option
    receiver: Option<TariCommsInboundReceiver<TariDanPayload>>,
    sender: Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)>,
    asset_public_key: PublicKey,
}

impl TariCommsInboundConnectionService {
    pub fn new(asset_public_key: PublicKey) -> Self {
        let (receiver, sender) = TariCommsInboundReceiver::new();
        Self {
            receiver: Some(receiver),
            sender,
            asset_public_key,
        }
    }

    pub fn clone_sender(&self) -> Sender<(CommsPublicKey, HotStuffMessage<TariDanPayload>)> {
        self.sender.clone()
    }

    pub fn take_receiver(&mut self) -> Option<TariCommsInboundReceiver<TariDanPayload>> {
        // Takes the receiver, can only be done once
        self.receiver.take()
    }

    pub async fn run(
        &mut self,
        _shutdown_signal: ShutdownSignal,
        inbound_stream: impl Stream<Item = Arc<PeerMessage>>,
    ) -> Result<(), DigitalAssetError> {
        let inbound_stream = inbound_stream.fuse();
        pin_mut!(inbound_stream);
        loop {
            futures::select! {
                message = inbound_stream.select_next_some() => {

                        self.forward_message(message).await?;

                }
                complete => {
                    dbg!("Tari inbound connector shutting down");
                    return Ok(());
                }
                // _ = shutdown_signal => {
                //     dbg!("Shutdown received");
                //     return Ok(())
                // }
            }
        }
    }

    async fn forward_message(&mut self, message: Arc<PeerMessage>) -> Result<(), DigitalAssetError> {
        // let from = message.authenticated_origin.as_ref().unwrap().clone();
        let from = message.source_peer.public_key.clone();
        let proto_message: proto::dan::HotStuffMessage = message.decode_message().unwrap();
        let hot_stuff_message: HotStuffMessage<TariDanPayload> = proto_message
            .try_into()
            .map_err(DigitalAssetError::InvalidPeerMessage)?;
        if hot_stuff_message.asset_public_key() == &self.asset_public_key {
            dbg!(&hot_stuff_message);
            self.sender.send((from, hot_stuff_message)).await.unwrap();
        } else {
            dbg!("filtered");
        }
        Ok(())
    }
}

// TODO: Perhaps this is a hack, and should be moved to a better structure. This struct exists to create a Sync+ Send
// inbound service that can be given to the consensus worker
pub struct TariCommsInboundReceiver<TPayload: Payload> {
    receiver: Receiver<(CommsPublicKey, HotStuffMessage<TPayload>)>,
}

impl<TPayload: Payload> TariCommsInboundReceiver<TPayload> {
    fn new() -> (Self, Sender<(CommsPublicKey, HotStuffMessage<TPayload>)>) {
        let (sender, receiver) = channel(1000);
        (Self { receiver }, sender)
    }
}
#[async_trait]
impl<TPayload: Payload> InboundConnectionService<CommsPublicKey, TPayload> for TariCommsInboundReceiver<TPayload> {
    async fn receive_message(&mut self) -> (CommsPublicKey, HotStuffMessage<TPayload>) {
        // TODO: handle errors
        self.receiver.recv().await.unwrap()
    }
}
