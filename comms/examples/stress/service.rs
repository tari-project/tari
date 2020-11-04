//  Copyright 2020, The Tari Project
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

use super::error::Error;
use crate::stress::{MAX_FRAME_SIZE, STRESS_PROTOCOL_NAME};
use bytes::{Buf, Bytes, BytesMut};
use futures::{
    channel::{mpsc, oneshot},
    stream,
    stream::Fuse,
    AsyncReadExt,
    AsyncWriteExt,
    SinkExt,
    StreamExt,
};
use rand::{rngs::OsRng, RngCore};
use std::{
    iter::repeat_with,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::{
    framing,
    message::{InboundMessage, OutboundMessage},
    peer_manager::{NodeId, Peer},
    protocol::{ProtocolEvent, ProtocolNotification},
    CommsNode,
    PeerConnection,
    Substream,
};
use tari_crypto::tari_utilities::hex::Hex;
use tokio::{sync::RwLock, task, task::JoinHandle, time};

pub fn start_service(
    comms_node: CommsNode,
    protocol_notif: mpsc::Receiver<ProtocolNotification<Substream>>,
    inbound_rx: mpsc::Receiver<InboundMessage>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
) -> (JoinHandle<Result<(), Error>>, mpsc::Sender<StressTestServiceRequest>)
{
    let node_identity = comms_node.node_identity();
    let (request_tx, request_rx) = mpsc::channel(1);

    println!(
        "Node credentials are {}::{} (local_listening_addr='{}')",
        node_identity.public_key().to_hex(),
        node_identity.public_address(),
        comms_node.listening_address(),
    );

    let service = StressTestService::new(request_rx, comms_node, protocol_notif, inbound_rx, outbound_tx);
    (task::spawn(service.start()), request_tx)
}

pub enum StressTestServiceRequest {
    BeginProtocol(Peer, StressProtocol, oneshot::Sender<Result<(), Error>>),
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
pub struct StressProtocol {
    pub kind: StressProtocolKind,
    pub num_messages: u32,
    pub message_size: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum StressProtocolKind {
    ContinuousSend,
    AlternatingSend,
    MessagingFlood,
}

impl StressProtocol {
    pub fn new(kind: StressProtocolKind, num_messages: u32, message_size: u32) -> Self {
        Self {
            kind,
            num_messages,
            message_size,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(5);
        match self.kind {
            StressProtocolKind::ContinuousSend => {
                data.push(0x01);
            },
            StressProtocolKind::AlternatingSend => {
                data.push(0x02);
            },
            StressProtocolKind::MessagingFlood => {
                data.push(0x03);
            },
        }

        data.extend_from_slice(&self.num_messages.to_be_bytes());
        data.extend_from_slice(&self.message_size.to_be_bytes());
        data
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 9 {
            return None;
        }
        let n = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let s = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let kind = match bytes[0] {
            0x01 => StressProtocolKind::ContinuousSend,
            0x02 => StressProtocolKind::AlternatingSend,
            0x03 => StressProtocolKind::MessagingFlood,
            _ => return None,
        };

        Some(Self::new(kind, n, s))
    }
}

struct StressTestService {
    request_rx: Fuse<mpsc::Receiver<StressTestServiceRequest>>,
    comms_node: CommsNode,
    protocol_notif: Fuse<mpsc::Receiver<ProtocolNotification<Substream>>>,
    shutdown: bool,

    inbound_rx: Arc<RwLock<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
}

impl StressTestService {
    pub fn new(
        request_rx: mpsc::Receiver<StressTestServiceRequest>,
        comms_node: CommsNode,
        protocol_notif: mpsc::Receiver<ProtocolNotification<Substream>>,
        inbound_rx: mpsc::Receiver<InboundMessage>,
        outbound_tx: mpsc::Sender<OutboundMessage>,
    ) -> Self
    {
        Self {
            request_rx: request_rx.fuse(),
            comms_node,
            protocol_notif: protocol_notif.fuse(),
            shutdown: false,
            inbound_rx: Arc::new(RwLock::new(inbound_rx)),
            outbound_tx,
        }
    }

    async fn start(mut self) -> Result<(), Error> {
        let mut events = self.comms_node.subscribe_connectivity_events().fuse();

        loop {
            futures::select! {
                event = events.select_next_some() => {
                    if let Ok(event) = event {
                        println!("{}", event);
                    }
                },

                request = self.request_rx.select_next_some() => {
                    if let Err(err) = self.handle_request(request).await {
                        println!("Error: {}", err);
                    }
                },

                notif = self.protocol_notif.select_next_some() => {
                    self.handle_protocol_notification(notif).await;
                },
            }

            if self.shutdown {
                break;
            }
        }

        self.comms_node.wait_until_shutdown().await;

        Ok(())
    }

    async fn handle_request(&mut self, request: StressTestServiceRequest) -> Result<(), Error> {
        use StressTestServiceRequest::*;
        match request {
            BeginProtocol(peer, protocol, reply) => self.begin_protocol(peer, protocol, reply).await?,
            Shutdown => {
                self.shutdown = true;
            },
        }

        Ok(())
    }

    async fn begin_protocol(
        &mut self,
        peer: Peer,
        protocol: StressProtocol,
        reply: oneshot::Sender<Result<(), Error>>,
    ) -> Result<(), Error>
    {
        let node_id = peer.node_id.clone();
        self.comms_node.peer_manager().add_peer(peer).await?;
        println!("Dialing peer `{}`...", node_id.short_str());
        let start = Instant::now();
        let conn = self.comms_node.connectivity().dial_peer(node_id).await?;
        println!("Dial completed successfully in {:.2?}", start.elapsed());
        let outbound_tx = self.outbound_tx.clone();
        let inbound_rx = self.inbound_rx.clone();
        task::spawn(async move {
            let _ = reply.send(start_initiator_protocol(conn, protocol, inbound_rx, outbound_tx).await);
        });

        Ok(())
    }

    async fn handle_protocol_notification(&mut self, notification: ProtocolNotification<Substream>) {
        match notification.event {
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                println!(
                    "Peer `{}` initiated `{}` protocol",
                    node_id.short_str(),
                    String::from_utf8_lossy(&notification.protocol)
                );

                task::spawn(start_responder_protocol(
                    node_id,
                    substream,
                    self.inbound_rx.clone(),
                    self.outbound_tx.clone(),
                ));
            },
        }
    }
}

async fn start_initiator_protocol(
    mut conn: PeerConnection,
    protocol: StressProtocol,

    inbound_rx: Arc<RwLock<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
) -> Result<(), Error>
{
    println!("Negotiating {:?} protocol...", protocol);
    let start = Instant::now();
    let substream = conn.open_substream(&STRESS_PROTOCOL_NAME).await?;
    println!("Negotiation completed in {:.0?}", start.elapsed());
    let mut substream = substream.stream;

    substream.write_all(&protocol.encode()).await?;
    substream.flush().await?;
    let mut framed = framing::canonical(substream, MAX_FRAME_SIZE);

    let start = Instant::now();
    match protocol.kind {
        StressProtocolKind::ContinuousSend => {
            let mut counter = 0u32;

            println!(
                "Sending {} messages ({} MiB) continuously",
                protocol.num_messages,
                protocol.num_messages * protocol.message_size / 1024 / 1024
            );
            let mut iter = stream::iter(
                repeat_with(|| {
                    counter += 1;
                    generate_message(counter, protocol.message_size as usize)
                })
                .take(protocol.num_messages as usize)
                .map(Ok),
            );
            framed.send_all(&mut iter).await?;
            framed.close().await?;
            println!("Done in {:.2?}. Closing substream.", start.elapsed());
        },
        StressProtocolKind::AlternatingSend => {
            let mut counter = 0u32;
            let num_loops = protocol.num_messages / 200;
            let num_to_send = num_loops * 100;
            println!(
                "Alternating between sending and receiving (sending {} messages/{} MiB) ",
                num_to_send,
                num_to_send * protocol.message_size / 1024 / 1024
            );
            let mut received = Vec::with_capacity(num_to_send as usize);
            for _ in 0..num_loops {
                let mut iter = stream::iter(
                    repeat_with(|| {
                        counter += 1;
                        generate_message(counter, protocol.message_size as usize)
                    })
                    .take(100)
                    .map(Ok),
                );
                framed.send_all(&mut iter).await?;

                // Read 100
                for _ in 0..100usize {
                    counter += 1;
                    let msg = framed.next().await.ok_or_else(|| Error::UnexpectedEof)??;
                    received.push(decode_msg(msg));
                }
            }
            println!("Received {} messages ", received.len());

            framed.close().await?;
            println!("Done in {:.2?}. Closing substream.", start.elapsed());
        },
        StressProtocolKind::MessagingFlood => {
            messaging_flood(conn.peer_node_id().clone(), protocol, inbound_rx, outbound_tx).await?;
            // Close to indicate we're done
            framed.close().await?;
        },
    }

    Ok(())
}

async fn start_responder_protocol(
    peer: NodeId,
    mut substream: Substream,

    inbound_rx: Arc<RwLock<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
) -> Result<(), Error>
{
    let mut buf = [0u8; 9];
    substream.read_exact(&mut buf).await?;
    let protocol = StressProtocol::decode(&buf).ok_or_else(|| Error::InvalidProtocolFrame)?;

    let framed = framing::canonical(substream, MAX_FRAME_SIZE);
    let (mut sink, mut stream) = framed.split();

    println!();
    println!("-------------------------------------------------");
    println!("Peer `{}` chose {:?}.", peer.short_str(), protocol,);
    println!("-------------------------------------------------");
    match protocol.kind {
        StressProtocolKind::ContinuousSend => {
            let mut received = vec![];
            let start = Instant::now();

            while let Some(Ok(msg)) = stream.next().await {
                received.push(decode_msg(msg));
            }

            println!(
                "[peer: {}] Protocol complete in {:.2?}. {} messages received",
                peer.short_str(),
                start.elapsed(),
                received.len()
            );

            match received.len() {
                v if v == protocol.num_messages as usize => {
                    println!("All messages received");
                },
                v => {
                    println!(
                        "Invalid number of messages received (expected {}, got {})",
                        protocol.num_messages, v
                    );
                },
            }
        },
        StressProtocolKind::AlternatingSend => {
            let mut received = vec![];
            let start = Instant::now();

            let mut counter = 0u32;
            let num_loops = protocol.num_messages / 200;
            println!(
                "[peer: {}] Expecting to send and receive {} messages",
                peer.short_str(),
                num_loops * 200
            );
            for _ in 0..num_loops {
                // Read 100
                for _ in 0..100usize {
                    counter += 1;
                    let msg = stream.next().await.ok_or_else(|| Error::UnexpectedEof)??;
                    received.push(decode_msg(msg));
                }

                // Send 100
                let mut iter = stream::iter(
                    repeat_with(|| {
                        counter += 1;
                        generate_message(counter, protocol.message_size as usize)
                    })
                    .take(100)
                    .map(Ok),
                );
                sink.send_all(&mut iter).await?;
            }

            // Wait for the stream to close
            let _ = stream.next().await;

            println!(
                "Protocol complete in {:.2?}. {} messages received, {} sent",
                start.elapsed(),
                received.len(),
                num_loops * 100
            );
        },
        StressProtocolKind::MessagingFlood => {
            messaging_flood(peer, protocol, inbound_rx, outbound_tx).await?;
            sink.close().await?;
        },
    }

    Ok(())
}

async fn messaging_flood(
    peer: NodeId,
    protocol: StressProtocol,
    inbound_rx: Arc<RwLock<mpsc::Receiver<InboundMessage>>>,
    mut outbound_tx: mpsc::Sender<OutboundMessage>,
) -> Result<(), Error>
{
    let start = Instant::now();
    let mut counter = 1u32;
    println!(
        "Sending and receiving {} messages ({} MiB each way) using the messaging protocol",
        protocol.num_messages,
        protocol.num_messages * protocol.message_size / 1024 / 1024
    );
    let outbound_task = task::spawn(async move {
        let mut iter = stream::iter(
            repeat_with(|| {
                counter += 1;

                println!("Send MSG {}", counter);
                OutboundMessage::new(peer.clone(), generate_message(counter, protocol.message_size as usize))
            })
            .take(protocol.num_messages as usize)
            .map(Ok),
        );
        outbound_tx.send_all(&mut iter).await?;
        time::delay_for(Duration::from_secs(5)).await;
        outbound_tx
            .send(OutboundMessage::new(peer.clone(), Bytes::from_static(&[0u8; 4])))
            .await?;
        Result::<_, Error>::Ok(())
    });

    let inbound_task = task::spawn(async move {
        let mut inbound_rx = inbound_rx.write().await;
        let mut msgs = vec![];
        loop {
            if let Some(msg) = inbound_rx.next().await {
                let msg_id = decode_msg(msg.body);
                println!("GOT MSG {}", msg_id);
                if msgs.len() == protocol.num_messages as usize {
                    // msg_id == 0 {
                    break;
                }
                msgs.push(msg_id);
            } else {
                break;
            }
        }
        msgs
    });

    outbound_task.await??;
    let msgs = inbound_task.await?;

    println!(
        "Received {}/{} messages in {:.0?}",
        msgs.len(),
        protocol.num_messages,
        start.elapsed()
    );
    Ok(())
}

fn generate_message(n: u32, size: usize) -> Bytes {
    let counter_bytes = n.to_be_bytes();
    let mut bytes = BytesMut::with_capacity(size);
    bytes.resize(size, 0);
    bytes[..4].copy_from_slice(&counter_bytes);
    OsRng.fill_bytes(&mut bytes[4..size]);
    bytes.freeze()
}

fn decode_msg<T: prost::bytes::Buf>(msg: T) -> u32 {
    let mut buf = [0u8; 4];
    msg.bytes().copy_to_slice(&mut buf);
    u32::from_be_bytes(buf)
}
