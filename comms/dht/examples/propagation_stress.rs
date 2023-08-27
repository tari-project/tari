//  Copyright 2022, The Taiji Project
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

mod propagation;
use std::{
    collections::VecDeque,
    convert::TryFrom,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use propagation::node;
use rand::{rngs::OsRng, Rng};
use taiji_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, CommsNode, NodeIdentity};
use taiji_comms_dht::{
    domain_message::OutboundDomainMessage,
    inbound::DecryptedDhtMessage,
    outbound::{MessageSendStates, OutboundEncryption},
    Dht,
};
use taiji_shutdown::Shutdown;
use tempfile::tempdir;
use tokio::{sync::mpsc, task};

use crate::propagation::prompt::{parse_from_short_str, user_prompt, SendMethod};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::args().any(|a| a == "--enable-tracing") {
        // Uncomment to enable tokio tracing via tokio-console
        // console_subscriber::init();
    } else {
        // env logger does not work with console subscriber enabled
        env_logger::init();
    }

    let tmp_path = tempdir()?;
    let shutdown = Shutdown::new();
    let node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        Multiaddr::empty(),
        PeerFeatures::COMMUNICATION_CLIENT,
    ));
    let port = OsRng.gen_range(9000u16..55000);
    let seed_peers = &[
        "c2eca9cf32261a1343e21ed718e79f25bfc74386e9305350b06f62047f519347::/onion3/\
         6yxqk2ybo43u73ukfhyc42qn25echn4zegjpod2ccxzr2jd5atipwzqd:18141",
        "42fcde82b44af1de95a505d858cb31a422c56c4ac4747fbf3da47d648d4fc346::/onion3/\
         2l3e7ysmihc23zybapdrsbcfg6omtjtfkvwj65dstnfxkwtai2fawtyd:18141",
        "50e6aa8f6c50f1b9d9b3d438dfd2a29cfe1f3e3a650bd9e6b1e10f96b6c38f4d::/onion3/\
         7s6y3cz5bnewlj5ypm7sekhgvqjyrq4bpaj5dyvvo7vxydj7hsmyf5ad:18141",
        "36a9df45e1423b5315ffa7a91521924210c8e1d1537ad0968450f20f21e5200d::/onion3/\
         v24qfheti2rztlwzgk6v4kdbes3ra7mo3i2fobacqkbfrk656e3uvnid:18141",
        "be128d570e8ec7b15c101ee1a56d6c56dd7d109199f0bd02f182b71142b8675f::/onion3/\
         ha422qsy743ayblgolui5pg226u42wfcklhc5p7nbhiytlsp4ir2syqd:18141",
        "3e0321c0928ca559ab3c0a396272dfaea705efce88440611a38ff3898b097217::/onion3/\
         sl5ledjoaisst6d4fh7kde746dwweuge4m4mf5nkzdhmy57uwgtb7qqd:18141",
        "b0f797e7413b39b6646fa370e8394d3993ead124b8ba24325c3c07a05e980e7e::/ip4/35.177.93.69/tcp/18189",
        "0eefb45a4de9484eca74846a4f47d2c8d38e76be1fec63b0112bd00d297c0928::/ip4/13.40.98.39/tcp/18189",
        "544ed2baed414307e119d12894e27f9ddbdfa2fd5b6528dc843f27903e951c30::/ip4/13.40.189.176/tcp/18189",
    ];
    let (node, dht, msg_in) = node::create(
        Some(node_identity),
        &tmp_path,
        None,
        port,
        seed_peers.as_slice(),
        shutdown.to_signal(),
    )
    .await?;

    spawn_received_message_logger(&node, msg_in);
    println!("Waiting to go online...");
    node.connectivity()
        .wait_for_connectivity(Duration::from_secs(100))
        .await?;
    loop {
        match prompt(&node, &dht).await {
            Ok(()) => {},
            Err(err) => {
                log::warn!("{}", err);
                continue;
            },
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}

async fn prompt(node: &CommsNode, dht: &Dht) -> anyhow::Result<()> {
    let mut outbound = dht.outbound_requester();
    let node_identity = node.node_identity();
    let opts = task::spawn_blocking(move || user_prompt(&node_identity)).await??;

    let mut send_states = Vec::with_capacity(opts.num_msgs);
    for i in 0..opts.num_msgs {
        let msg = OutboundDomainMessage::new(&999, PropagationMessage::new(u32::try_from(i).unwrap(), opts.msg_size));
        let states = match opts.send_method {
            SendMethod::Direct => outbound
                .send_direct_node_id(opts.peer.node_id.clone(), msg, "Example stress".to_string())
                .await
                .map(MessageSendStates::from)?,
            SendMethod::Propagated => {
                outbound
                    .propagate(
                        opts.peer.public_key.clone().into(),
                        OutboundEncryption::EncryptFor(Box::new(opts.peer.public_key.clone())),
                        // Don't send directly to peer
                        vec![opts.peer.node_id.clone()],
                        msg,
                    )
                    .await?
            },
        };
        send_states.push(states);
        println!("#{} queued for sending at {}", i, chrono::Local::now().to_rfc2822());
    }

    for states in send_states {
        states.wait_all().await;
    }

    Ok(())
}

#[derive(Clone, prost::Message)]
struct PropagationMessage {
    #[prost(uint32, tag = "1")]
    id: u32,
    #[prost(bytes, tag = "2")]
    payload: Vec<u8>,
    #[prost(string, tag = "3")]
    ts: String,
}

impl PropagationMessage {
    pub fn new(id: u32, data_size: usize) -> Self {
        Self {
            id,
            payload: vec![1u8; data_size],
            ts: chrono::Utc::now().to_rfc2822(),
        }
    }
}

fn spawn_received_message_logger(_node: &CommsNode, mut msg_in: mpsc::Receiver<DecryptedDhtMessage>) {
    println!("Waiting for messages...");
    task::spawn(async move {
        let mut last_msg_ts = Instant::now();
        let mut total_received = 0;
        let mut tps = VecDeque::with_capacity(50);
        while let Some(msg) = msg_in.recv().await {
            total_received += 1;
            let body = msg.success().unwrap();
            let msg = body.decode_part::<PropagationMessage>(1).unwrap().unwrap();
            let dt = chrono::DateTime::parse_from_rfc2822(&msg.ts).unwrap();
            let since = chrono::Utc::now().signed_duration_since(dt);
            tps.push_front(Instant::now());
            println!(
                "#{} ({} bytes) received {:.2?} after the message was queued. Total received: {}, last msg: {}s",
                msg.id,
                msg.payload.len(),
                since.to_std().unwrap_or_default(),
                total_received,
                last_msg_ts.elapsed().as_secs_f32()
            );
            last_msg_ts = Instant::now();
        }
    });
}
