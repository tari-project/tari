//  Copyright 2022. The Tari Project
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

use std::time::Duration;

use futures::{future, SinkExt, StreamExt};
use tari_comms::{
    framing,
    protocol::{ProtocolEvent, ProtocolId, ProtocolNotificationRx},
    transports::TcpTransport,
    BytesMut,
    CommsNode,
    Substream,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::unpack_enum;
use tokio::{sync::mpsc, task, time::Instant};

use crate::tests::helpers::create_comms;

const PROTOCOL_NAME: &[u8] = b"test/dummy/protocol";

pub async fn spawn_node(signal: ShutdownSignal) -> (CommsNode, ProtocolNotificationRx<Substream>) {
    let (notif_tx, notif_rx) = mpsc::channel(1);
    let mut comms = create_comms(signal)
        .add_protocol(&[ProtocolId::from_static(PROTOCOL_NAME)], &notif_tx)
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

    (comms, notif_rx)
}

async fn run_stress_test(num_substreams: usize, num_iterations: usize, payload_size: usize, frame_size: usize) {
    let shutdown = Shutdown::new();
    let (node1, _) = spawn_node(shutdown.to_signal()).await;
    let (node2, mut notif_rx) = spawn_node(shutdown.to_signal()).await;

    node1
        .peer_manager()
        .add_peer(node2.node_identity().to_peer())
        .await
        .unwrap();

    let mut conn = node1
        .connectivity()
        .dial_peer(node2.node_identity().node_id().clone())
        .await
        .unwrap();

    let sample = {
        let mut buf = BytesMut::with_capacity(payload_size);
        buf.fill(1);
        buf.freeze()
    };

    task::spawn({
        let sample = sample.clone();
        async move {
            while let Some(event) = notif_rx.recv().await {
                unpack_enum!(ProtocolEvent::NewInboundSubstream(_n, remote_substream) = event.event);
                let mut remote_substream = framing::canonical(remote_substream, frame_size);

                task::spawn({
                    let sample = sample.clone();
                    async move {
                        let mut count = 0;
                        while let Some(r) = remote_substream.next().await {
                            r.unwrap();
                            count += 1;
                            remote_substream.send(sample.clone()).await.unwrap();

                            if count == num_iterations {
                                break;
                            }
                        }

                        assert_eq!(count, num_iterations);
                    }
                });
            }
        }
    });

    let mut substreams = Vec::with_capacity(num_substreams);
    for _ in 0..num_substreams {
        let substream = conn
            .open_framed_substream(&ProtocolId::from_static(PROTOCOL_NAME), frame_size)
            .await
            .unwrap();
        substreams.push(substream);
    }

    let tasks = substreams
        .into_iter()
        .enumerate()
        .map(|(id, mut substream)| {
            let sample = sample.clone();
            task::spawn(async move {
                let mut count = 1;
                // Send first to get the ball rolling
                substream.send(sample.clone()).await.unwrap();
                let mut total_time = Duration::from_secs(0);
                while let Some(r) = substream.next().await {
                    r.unwrap();
                    count += 1;
                    let t = Instant::now();
                    substream.send(sample.clone()).await.unwrap();
                    total_time += t.elapsed();
                    if count == num_iterations {
                        break;
                    }
                }

                println!("[task {}] send time = {:.2?}", id, total_time,);
                assert_eq!(count, num_iterations);
                total_time
            })
        })
        .collect::<Vec<_>>();

    let send_latencies = future::join_all(tasks)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let avg = send_latencies.iter().sum::<Duration>().as_millis() / send_latencies.len() as u128;
    println!("avg t = {}ms", avg);
}

#[tokio::test]
async fn many_at_frame_limit() {
    const NUM_SUBSTREAMS: usize = 20;
    const NUM_ITERATIONS_PER_STREAM: usize = 100;
    const MAX_FRAME_SIZE: usize = 4 * 1024 * 1024;
    const PAYLOAD_SIZE: usize = 4 * 1024 * 1024;
    run_stress_test(NUM_SUBSTREAMS, NUM_ITERATIONS_PER_STREAM, PAYLOAD_SIZE, MAX_FRAME_SIZE).await;
}
