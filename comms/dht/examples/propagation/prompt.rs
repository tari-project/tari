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

use std::{io::stdin, str::FromStr, sync::Arc};

use anyhow::anyhow;
use taiji_comms::{
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFeatures},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_utilities::hex::Hex;

macro_rules! or_continue {
    ($expr:expr, $($arg:tt)*) => {
        match $expr {
            Some(v) => v,
            None => {
                println!($($arg),*);
                continue;
            },
        }
    };
    ($expr:expr) => {
        or_continue!($expr, "Invalid input")
    }
}

macro_rules! prompt {
    ($($arg:tt)*) => {{
        use std::io::Write;
        println!($($arg)*);
        print!("> ");
        std::io::stdout().flush().unwrap();
    }}
}

pub fn user_prompt(node_identity: &Arc<NodeIdentity>) -> anyhow::Result<PropagationOpts> {
    fn read_line<T: FromStr>(default: T) -> Option<T> {
        let mut buf = String::new();
        stdin().read_line(&mut buf).unwrap();
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            Some(default)
        } else {
            trimmed.parse().ok()
        }
    }

    loop {
        println!("{}::{:?}", node_identity.public_key(), node_identity.public_addresses());
        println!("{}", node_identity);
        prompt!("Enter the peer:");
        let peer = or_continue!(read_line(String::new()));
        let peer = or_continue!(parse_from_short_str(&peer, Default::default()));
        prompt!("Enter the number of messages to send (default: 1,000)");
        let num_msgs = or_continue!(read_line::<usize>(1_000));
        prompt!("Enter the message size (default: 256 bytes)");
        let msg_size = or_continue!(read_line::<usize>(256));
        prompt!("Direct (d) or propagation (p - default)?");
        let send_method = or_continue!(read_line::<String>("p".to_string()));

        prompt!(
            "About to send {} messages totalling {} MiB. Ok? (y/n)",
            num_msgs,
            (msg_size * num_msgs) / 1024 / 1024
        );
        let ans = or_continue!(read_line::<char>('n'));
        if ans.to_ascii_uppercase() != 'Y' {
            continue;
        }

        break Ok(PropagationOpts {
            peer,
            num_msgs,
            msg_size,
            send_method: send_method.parse().unwrap(),
        });
    }
}

#[derive(Debug, Clone)]
pub struct PropagationOpts {
    pub peer: Peer,
    pub num_msgs: usize,
    pub msg_size: usize,
    pub send_method: SendMethod,
}

#[derive(Debug, Clone, Copy)]
pub enum SendMethod {
    Direct,
    Propagated,
}

impl FromStr for SendMethod {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "d" | "direct" => Ok(SendMethod::Direct),
            "p" | "propagate" | "propagated" => Ok(SendMethod::Propagated),
            _ => Err(anyhow!("bad send method {}", s)),
        }
    }
}

pub fn parse_from_short_str(s: &str, features: PeerFeatures) -> Option<Peer> {
    let mut split = s.splitn(2, "::");
    let pk = split.next().and_then(|s| CommsPublicKey::from_hex(s).ok())?;
    let node_id = NodeId::from_key(&pk);
    let address = split.next().and_then(|s| s.parse::<Multiaddr>().ok())?;
    Some(Peer::new(
        pk,
        node_id,
        MultiaddressesWithStats::from_addresses_with_source(vec![address], &PeerAddressSource::Config),
        Default::default(),
        features,
        Default::default(),
        "taiji/stress-test/1.0".to_string(),
    ))
}
