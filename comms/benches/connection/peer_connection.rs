//  Copyright 2019 The Tari Project
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

use criterion::Criterion;

use std::{
    iter::repeat,
    sync::mpsc::{channel, sync_channel, Sender, SyncSender},
    thread,
    time::Duration,
};
use tari_comms::{
    connection::{
        peer_connection::PeerConnectionContext,
        Connection,
        Direction,
        InprocAddress,
        NetAddress,
        PeerConnection,
        PeerConnectionContextBuilder,
        ZmqContext,
    },
    message::FrameSet,
};

const BENCH_SOCKET_ADDRESS: &'static str = "127.0.0.1:9999";

/// Set the allocated stack size for each WorkerTask thread
const THREAD_STACK_SIZE: usize = 16 * 1024; // 16kb

lazy_static! {
    static ref DUMMY_DATA: FrameSet = vec![
        repeat(88u8).take(512 * 1024).collect::<Vec<u8>>(),
        repeat(99u8).take(512 * 1024).collect::<Vec<u8>>(),
    ]; // 1 MiB
}

/// Task messages for the consumer thread
enum WorkerTask {
    /// Receive a message
    Receive,
    /// Receive a message and then send it back
    ReceiveSend,
    /// Exit the loop (thread terminates)
    Exit,
}

/// Build a context for the connections to be used in the benchmark
fn build_context(
    ctx: &ZmqContext,
    dir: Direction,
    addr: &NetAddress,
    consumer_addr: &InprocAddress,
) -> PeerConnectionContext
{
    PeerConnectionContextBuilder::new()
        .set_peer_identity("benchmark")
        .set_direction(dir)
        .set_max_msg_size(512 * 1024)
        .set_address(addr.clone())
        .set_message_sink_address(consumer_addr.clone())
        .set_context(ctx)
        .build()
        .unwrap()
}

/// Start a consumer thread.
/// This listens for control messages, execute the given task and signal when it's done (`signal` field)
fn start_message_sink_consumer(
    ctx: &ZmqContext,
    addr: &InprocAddress,
    peer_conn: &PeerConnection,
    signal: Sender<()>,
) -> SyncSender<WorkerTask>
{
    let ctx = ctx.clone();
    let addr = addr.clone();
    let peer_conn = peer_conn.clone();
    let (tx, rx) = sync_channel(2);
    thread::Builder::new()
        .name("peer-connection-consumer-thread".to_string())
        .stack_size(THREAD_STACK_SIZE)
        .spawn(move || {
            let conn = Connection::new(&ctx, Direction::Inbound).establish(&addr).unwrap();
            loop {
                match rx.recv().unwrap() {
                    WorkerTask::Receive => {
                        conn.receive(1000).unwrap();
                        signal.send(()).unwrap();
                    },
                    WorkerTask::ReceiveSend => {
                        let data = conn.receive(1000).unwrap();
                        peer_conn.send(data).unwrap();
                        signal.send(()).unwrap();
                    },
                    WorkerTask::Exit => break,
                }
            }
        });
    tx
}

/// Benchmark for PeerConnnection
///
/// ## Setup Phase
///
/// 1. Two peer connections (one inbound and one outbound) are "booted up"
/// 2. Consumer workers are started up and set to listen for messages from the peer connection
///
/// ## Run phase
///
/// 1. Peer 1's worker is told to go into ReceiveSend mode
/// 2. Peer 2's worker is told to go into Receive mode
/// 3. Peer 2 (outbound) sends DUMMY_DATA to peer 1 (inbound)
/// 4. Peer 1 receives the message and sends it back to Peer 2 and signals done
/// 5. Peer 2 receives the data and signals done
/// 6. Once both have completed their work, the next iteration can begin
///
/// ## Teardown phase
///
/// 1. Both consumer threads are given the Exit task
/// 2. All connections go out of scope (i.e. are dropped)
fn bench_peer_connection(c: &mut Criterion) {
    // Setup
    let ctx = ZmqContext::new();
    let addr = BENCH_SOCKET_ADDRESS.parse::<NetAddress>().unwrap();
    let consumer1 = InprocAddress::random();
    let consumer2 = InprocAddress::random();

    let p1_ctx = build_context(&ctx, Direction::Inbound, &addr, &consumer1);
    let p2_ctx = build_context(&ctx, Direction::Outbound, &addr, &consumer2);

    // Start peer connections on either end
    let mut p1 = PeerConnection::connect();
    p1.start(p1_ctx).unwrap();

    let mut p2 = PeerConnection::connect();
    p2.start(p2_ctx).unwrap();

    let (done1_tx, done1_rx) = channel();
    let (done2_tx, done2_rx) = channel();

    // Start consumers
    // These act as a threaded worker which consumes messages from peer connections
    let signal1 = start_message_sink_consumer(&ctx, &consumer1, &p1, done1_tx);
    let signal2 = start_message_sink_consumer(&ctx, &consumer2, &p2, done2_tx);

    // Duplicates which won't be moved into the bench function
    let dup_signal1 = signal1.clone();
    let dup_signal2 = signal2.clone();

    p1.wait_listening_or_failure(&Duration::from_millis(1000)).unwrap();
    p2.wait_connected_or_failure(&Duration::from_millis(1000)).unwrap();

    c.bench_function("peer_connection: send/recv", move |b| {
        // Benchmark
        b.iter(|| {
            // Signal p2 to receive and send back
            signal1.send(WorkerTask::ReceiveSend).unwrap();
            // Signal p1 to receive
            signal2.send(WorkerTask::Receive).unwrap();

            // Send to p2
            p2.send(DUMMY_DATA.to_vec()).unwrap();

            // Wait for done signals
            done1_rx.recv().unwrap();
            done2_rx.recv().unwrap();
        });
    });

    // Teardown
    dup_signal1.send(WorkerTask::Exit).unwrap();
    dup_signal2.send(WorkerTask::Exit).unwrap();
}

criterion_group!(
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500));
    targets = bench_peer_connection,
);
