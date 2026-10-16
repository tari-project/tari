//  Copyright 2026. The Tari Project
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
#![allow(clippy::indexing_slicing)]

use std::{convert::identity, time::Duration};

use tari_comms::{
    CommsNode,
    RefKind,
    message::{InboundMessage, OutboundMessage},
    pipeline,
    pipeline::SinkService,
    protocol::{ProtocolId, messaging::MessagingProtocolExtension},
    transports::TcpTransport,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    io::AsyncWriteExt,
    sync::{broadcast, mpsc},
    time,
};

use crate::tests::{helpers::create_comms, substream_stress::spawn_node};

static MESSAGING_PROTOCOL_ID: ProtocolId = ProtocolId::from_static(b"test/messaging");

async fn spawn_messaging_node(
    signal: ShutdownSignal,
) -> (
    CommsNode,
    mpsc::Receiver<InboundMessage>,
    mpsc::UnboundedSender<OutboundMessage>,
) {
    let (inbound_tx, inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
    let (event_tx, _) = broadcast::channel(10);
    let mut comms = create_comms(signal)
        .add_protocol_extension(MessagingProtocolExtension::new(
            MESSAGING_PROTOCOL_ID.clone(),
            event_tx,
            pipeline::Builder::new()
                .with_outbound_pipeline(outbound_rx, identity)
                .max_concurrent_inbound_tasks(1)
                .with_inbound_pipeline(SinkService::new(inbound_tx))
                .build(),
        ))
        .spawn_with_transport(TcpTransport::new())
        .await
        .unwrap();

    let address = comms
        .connection_manager_requester()
        .wait_until_listening()
        .await
        .unwrap();
    comms
        .node_identity()
        .set_public_addresses(vec![address.bind_address().clone()]);

    (comms, inbound_rx, outbound_tx)
}

#[tokio::test]
async fn oversized_frame_bans_sender() {
    let shutdown = Shutdown::new();
    let (sender, _) = spawn_node(shutdown.to_signal()).await;
    let (receiver, _inbound_rx, _outbound_tx) = spawn_messaging_node(shutdown.to_signal()).await;

    sender
        .peer_manager()
        .add_or_update_peer(receiver.node_identity().to_peer())
        .await
        .unwrap();
    let mut conn = sender
        .connectivity()
        .dial_peer(receiver.node_identity().node_id().clone(), RefKind::Weak)
        .await
        .unwrap();

    // A 9 MiB length header, over the 8 MiB messaging frame limit
    let mut substream = conn.open_substream(&MESSAGING_PROTOCOL_ID).await.unwrap().stream;
    let oversized_len = u32::try_from(9 * 1024 * 1024).unwrap();
    substream.write_all(&oversized_len.to_be_bytes()).await.unwrap();
    substream.flush().await.unwrap();

    let sender_node_id = sender.node_identity().node_id().clone();
    time::timeout(Duration::from_secs(10), async {
        // The receiver may not have stored the sender's peer record yet, which reads as not banned
        while !receiver
            .peer_manager()
            .is_peer_banned(&sender_node_id)
            .await
            .unwrap_or(false)
        {
            time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the peer that sent an oversized frame was not banned");

    shutdown.trigger();
}
