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

mod stress;
use stress::{error::Error, prompt::user_prompt};

use crate::stress::{node, prompt::parse_from_short_str, service, service::StressTestServiceRequest};
use futures::{channel::oneshot, future, future::Either, SinkExt};
use std::{env, net::Ipv4Addr, path::Path, process, sync::Arc, time::Duration};
use tari_crypto::tari_utilities::message_format::MessageFormat;
use tempfile::Builder;
use tokio::time;

#[tokio_macros::main_basic(max_threads = 1)]
async fn main() {
    env_logger::init();
    match run().await {
        Ok(_) | Err(Error::UserQuit) => {},
        Err(err) => {
            println!("{error:?}: {error}", error = err);
            process::exit(1);
        },
    }
}

fn remove_arg(args: &mut Vec<String>, item: &str) -> Option<usize> {
    if let Some(pos) = args.iter().position(|s| s == item) {
        args.remove(pos);
        Some(pos)
    } else {
        None
    }
}

async fn run() -> Result<(), Error> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();

    let is_tcp = remove_arg(&mut args, "--tcp").is_some();

    let mut rate_limit = 100_000;
    if let Some(pos) = remove_arg(&mut args, "--rate-limit") {
        rate_limit = args.remove(pos).parse().expect("Unable to parse rate limit");
    }
    println!("Rate limit set to: {}/s", rate_limit);

    let mut public_ip = None;
    if let Some(pos) = remove_arg(&mut args, "--public-ip") {
        public_ip = Some(args.remove(pos).parse::<Ipv4Addr>().unwrap());
    }

    let mut tor_identity_path = None;
    if let Some(pos) = remove_arg(&mut args, "--tor-identity") {
        tor_identity_path = Some(args.remove(pos));
    }

    let mut node_identity_path = None;
    if let Some(pos) = remove_arg(&mut args, "--identity") {
        node_identity_path = Some(args.remove(pos));
    }

    let mut peer = None;
    if let Some(pos) = remove_arg(&mut args, "--peer") {
        peer = Some(args.remove(pos));
    }

    let mut port = 9098;
    if let Some(pos) = remove_arg(&mut args, "--port") {
        port = args.remove(pos).parse().expect("Unable to parse port");
    }

    println!("Initializing...",);

    let tor_identity = tor_identity_path.as_ref().and_then(load_json);
    let node_identity = node_identity_path.as_ref().and_then(load_json).map(Arc::new);

    let temp_dir = Builder::new().prefix("stress-test").tempdir().unwrap();
    let (comms_node, protocol_notif) =
        node::create(node_identity, temp_dir.as_ref(), public_ip, port, tor_identity, is_tcp).await?;
    if let Some(node_identity_path) = node_identity_path.as_ref() {
        save_json(comms_node.node_identity_ref(), node_identity_path)?;
    }
    if !is_tcp {
        if let Some(tor_identity_path) = tor_identity_path.as_ref() {
            save_json(comms_node.hidden_service().unwrap().tor_identity(), tor_identity_path)?;
        }
    }

    println!("Stress test service started!");
    let (handle, mut requester) = service::start_service(comms_node, protocol_notif, rate_limit);

    let mut last_peer = peer.as_ref().and_then(parse_from_short_str);

    loop {
        let (peer, selected_protocol) = user_prompt(last_peer.take())?;
        last_peer = Some(peer.clone());
        let (reply_tx, reply_rx) = oneshot::channel();
        requester
            .send(StressTestServiceRequest::BeginProtocol(
                peer,
                selected_protocol,
                reply_tx,
            ))
            .await
            .unwrap();

        let ctrl_c = tokio::signal::ctrl_c();
        futures::pin_mut!(ctrl_c);
        match future::select(reply_rx, ctrl_c).await {
            Either::Left((result, _)) => {
                println!("Stress test complete");
                // TODO
                result??;
            },
            Either::Right((_, _)) => {
                println!("SIGINT caught. Waiting for service to exit");
                break;
            },
        }
    }

    println!("Stress test service is shutting down...");
    requester.send(StressTestServiceRequest::Shutdown).await.unwrap();
    time::timeout(Duration::from_secs(2), handle).await???;

    Ok(())
}

fn save_json<P: AsRef<Path>, T: MessageFormat>(obj: &T, path: P) -> Result<(), Error> {
    let json = obj.to_json()?;
    std::fs::write(path, json).map_err(Into::into)
}

fn load_json<P: AsRef<Path>, T: MessageFormat>(path: P) -> Option<T> {
    if path.as_ref().exists() {
        let contents = std::fs::read_to_string(path).unwrap();
        Some(T::from_json(&contents).unwrap())
    } else {
        None
    }
}
