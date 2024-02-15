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
#![cfg(feature = "rpc")]

// Run as normal, --nocapture for some extra output
// cargo test --package tari_comms --test rpc_stress run  --all-features --release -- --exact --nocapture

use std::{future::Future, time::Duration};

use futures::{future, StreamExt};
use tari_comms::{
    protocol::rpc::{RpcClient, RpcServer},
    transports::TcpTransport,
    CommsNode,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{task, time::Instant};

use crate::tests::{
    greeting_service::{GreetingClient, GreetingServer, GreetingService, StreamLargeItemsRequest},
    helpers::create_comms,
};

async fn spawn_node(signal: ShutdownSignal) -> CommsNode {
    let rpc_server = RpcServer::builder()
        .with_unlimited_simultaneous_sessions()
        .finish()
        .add_service(GreetingServer::new(GreetingService::default()));

    let mut comms = create_comms(signal)
        .add_rpc_server(rpc_server)
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

    comms
}

async fn run_stress_test(test_params: Params) {
    let Params {
        num_tasks,
        num_concurrent_sessions,
        deadline,
        payload_size,
        num_items,
    } = test_params;

    let time = Instant::now();
    assert!(
        num_tasks >= num_concurrent_sessions,
        "concurrent tasks must be more than concurrent sessions, otherwise the (lazy) pool wont make the given number \
         of sessions"
    );
    println!(
        "RPC stress test will transfer a total of {} MiB of data",
        (num_tasks * payload_size * num_items) / (1024 * 1024)
    );
    log::info!(
        "RPC stress test will transfer a total of {} MiB of data",
        (num_tasks * payload_size * num_items) / (1024 * 1024)
    );

    let shutdown = Shutdown::new();
    let node1 = spawn_node(shutdown.to_signal()).await;
    let node2 = spawn_node(shutdown.to_signal()).await;

    node1
        .peer_manager()
        .add_peer(node2.node_identity().to_peer())
        .await
        .unwrap();

    let conn1_2 = node1
        .connectivity()
        .dial_peer(node2.node_identity().node_id().clone())
        .await
        .unwrap();

    let client_pool = conn1_2.create_rpc_client_pool::<GreetingClient>(
        num_concurrent_sessions,
        RpcClient::builder().with_deadline(deadline),
    );

    let mut tasks = Vec::with_capacity(num_tasks);
    for i in 0..num_tasks {
        let pool = client_pool.clone();
        tasks.push(task::spawn(async move {
            let mut client = pool.get().await.unwrap();
            // let mut stream = client
            //     .get_greetings(GreetingService::DEFAULT_GREETINGS.len() as u32)
            //     .await
            //     .unwrap();
            // let mut count = 0;
            // while let Some(Ok(_)) = stream.next().await {
            //     count += 1;
            // }
            // assert_eq!(count, GreetingService::DEFAULT_GREETINGS.len());

            // let err = client.slow_response(5).await.unwrap_err();
            // unpack_enum!(RpcError::RequestFailed(err) = err);
            // assert_eq!(err.status_code(), RpcStatusCode::Timeout);

            // let msg = client.reply_with_msg_of_size(1024).await.unwrap();
            // assert_eq!(msg.len(), 1024);

            let time = std::time::Instant::now();
            log::info!("[{}] start {:.2?}", i, time.elapsed(),);
            let mut stream = client
                .stream_large_items(StreamLargeItemsRequest {
                    id: i as u64,
                    num_items: num_items as u64,
                    item_size: payload_size as u64,
                    delay_ms: 0,
                })
                .await
                .unwrap();

            log::info!("[{}] got stream {:.2?}", i, time.elapsed());
            let mut count = 0;
            while let Some(r) = stream.next().await {
                count += 1;
                log::info!(
                    "[{}] (count = {}) consuming stream {:.2?} : {}",
                    i,
                    count,
                    time.elapsed(),
                    r.as_ref().err().map(ToString::to_string).unwrap_or_else(String::new)
                );

                let _result = r.unwrap();
            }
            assert_eq!(count, num_items);
        }));
    }

    future::join_all(tasks).await.into_iter().for_each(Result::unwrap);

    log::info!("Stress test took {:.2?}", time.elapsed());
}

struct Params {
    pub num_tasks: usize,
    pub num_concurrent_sessions: usize,
    pub deadline: Duration,
    pub payload_size: usize,
    pub num_items: usize,
}

async fn quick() {
    run_stress_test(Params {
        num_tasks: 10,
        num_concurrent_sessions: 10,
        deadline: Duration::from_secs(5),
        payload_size: 1024,
        num_items: 10,
    })
    .await;
}

async fn basic() {
    run_stress_test(Params {
        num_tasks: 10,
        num_concurrent_sessions: 5,
        deadline: Duration::from_secs(15),
        payload_size: 1024 * 1024,
        num_items: 4,
    })
    .await;
}

async fn many_small_messages() {
    run_stress_test(Params {
        num_tasks: 10,
        num_concurrent_sessions: 10,
        deadline: Duration::from_secs(5),
        payload_size: 1024,
        num_items: 10 * 1024,
    })
    .await;
}

async fn few_large_messages() {
    run_stress_test(Params {
        num_tasks: 10,
        num_concurrent_sessions: 10,
        deadline: Duration::from_secs(5),
        payload_size: 1024 * 1024,
        num_items: 10,
    })
    .await;
}

async fn payload_limit() {
    run_stress_test(Params {
        num_tasks: 50,
        num_concurrent_sessions: 10,
        deadline: Duration::from_secs(20),
        payload_size: 4 * 1024 * 1024 - 100,
        num_items: 2,
    })
    .await;
}

// async fn high_contention() {
//     run_stress_test(Params {
//         num_tasks: 1000,
//         num_concurrent_sessions: 10,
//         deadline: Duration::from_secs(15),
//         payload_size: 1024 * 1024,
//         num_items: 4,
//     })
//     .await;
// }
//
// async fn high_concurrency() {
//     run_stress_test(Params {
//         num_tasks: 1000,
//         num_concurrent_sessions: 1000,
//         deadline: Duration::from_secs(15),
//         payload_size: 1024 * 1024,
//         num_items: 4,
//     })
//     .await;
// }
//
// async fn high_contention_high_concurrency() {
//     run_stress_test(Params {
//         num_tasks: 2000,
//         num_concurrent_sessions: 1000,
//         deadline: Duration::from_secs(15),
//         payload_size: 1024 * 1024,
//         num_items: 4,
//     })
//     .await;
// }

#[tokio::test]
async fn run() {
    // let _ = env_logger::try_init();
    log_timing("quick", quick()).await;
    if option_env!["COMMS_SKIP_LONG_RUNNING_STRESS_TESTS"].is_none() {
        log_timing("basic", basic()).await;
        log_timing("many_small_messages", many_small_messages()).await;
        log_timing("few_large_messages", few_large_messages()).await;
    }
    log_timing("payload_limit", payload_limit()).await;
    // log_timing("high_contention", high_contention()).await;
    // log_timing("high_concurrency", high_concurrency()).await;
    // log_timing("high_contention_high_concurrency", high_contention_high_concurrency()).await;
}

async fn log_timing<R, F: Future<Output = R>>(name: &str, fut: F) -> R {
    let t = Instant::now();
    println!("'{}' is running...", name);
    let ret = fut.await;
    let elapsed = t.elapsed();
    println!("'{}' completed in {:.2}s", name, elapsed.as_secs_f32());
    ret
}
