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
use crate::stress::service::{StressProtocol, StressProtocolKind};
use std::{cmp::min, io::stdin, str::FromStr};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer},
    types::CommsPublicKey,
};
use tari_crypto::tari_utilities::hex::Hex;

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

pub fn user_prompt(default_peer: Option<Peer>) -> Result<(Peer, StressProtocol), Error> {
    fn read_line<T: FromStr>(default: T) -> Option<T> {
        let mut buf = String::new();
        stdin().read_line(&mut buf).unwrap();
        let trimmed = buf.trim();
        if trimmed.len() == 0 {
            Some(default)
        } else {
            trimmed.parse().ok()
        }
    }

    let mut current_peer = default_peer;

    loop {
        let peer_str = current_peer.as_ref().map(to_short_str);
        println!("Select a stress test to perform:");
        println!("1) Continuous Send (Server sends 'n' messages to client)");
        println!("2) Alternating Send (Server and client alternate send and receiving)");
        println!("3) Burst Send (Server sends 'n' messages in bursts of 'm' to client)");
        println!();
        println!(
            "p) Set peer (current: {})",
            peer_str.as_ref().map(|s| s.as_str()).unwrap_or("<None>")
        );
        println!("q) Quit");
        prompt!();
        let opt = or_continue!(read_line::<char>(' '));

        let protocol = match opt {
            '1' => {
                prompt!("Enter the number of messages to send (default: 1,000,000)");
                let n = or_continue!(read_line::<u32>(1_000_000));
                prompt!("Enter the message size (default: 256 bytes)");
                let msg_size = or_continue!(read_line::<u32>(256));
                StressProtocol::new(StressProtocolKind::ContinuousSend, n, n, msg_size)
            },
            '2' => {
                prompt!("Enter the number of messages to send (default: 1,000,000)");
                let n = or_continue!(read_line::<u32>(1_000_000));
                prompt!("Enter the message size (default: 256 bytes)");
                let msg_size = or_continue!(read_line::<u32>(256));
                StressProtocol::new(StressProtocolKind::AlternatingSend, n, n, msg_size)
            },
            '3' => {
                prompt!("Enter the number of messages to send (default: 10,000)");
                let n = or_continue!(read_line::<u32>(10_000));
                prompt!("Enter the number of messages to include in a burst (default: 50)");
                let b = min(or_continue!(read_line::<u32>(50)), n);
                prompt!("Enter the message size (default: 256 bytes)");
                let msg_size = or_continue!(read_line::<u32>(256));
                StressProtocol::new(StressProtocolKind::BurstSend, n, b, msg_size)
            },
            'p' => {
                prompt!(
                    "Set peer (default: {})",
                    peer_str.as_ref().map(|s| s.as_str()).unwrap_or("<None>")
                );
                let peer = or_continue!(read_line(peer_str.unwrap_or_else(String::new)));
                if peer.is_empty() {
                    continue;
                }
                current_peer = Some(or_continue!(parse_from_short_str(&peer), "No peer selected"));
                continue;
            },
            'q' => break Err(Error::UserQuit),
            ' ' => continue,
            v => {
                println!("Invalid selection '{}'", v);
                continue;
            },
        };

        if current_peer.is_none() {
            prompt!(
                "Set peer (default: {})",
                peer_str.as_ref().map(|s| s.as_str()).unwrap_or("<None>")
            );
            let peer = or_continue!(read_line(peer_str.unwrap_or_else(String::new)));
            if peer.is_empty() {
                continue;
            }
            current_peer = Some(or_continue!(parse_from_short_str(&peer), "No peer selected"));
        }

        break Ok((current_peer.unwrap(), protocol));
    }
}

pub fn to_short_str(peer: &Peer) -> String {
    format!("{}::{}", peer.public_key, peer.addresses.first().unwrap())
}

pub fn parse_from_short_str(s: &String) -> Option<Peer> {
    let mut split = s.splitn(2, "::");
    let pk = split.next().and_then(|s| CommsPublicKey::from_hex(s).ok())?;
    let node_id = NodeId::from_key(&pk).ok()?;
    let address = split.next().and_then(|s| s.parse::<Multiaddr>().ok())?;
    Some(Peer::new(
        pk,
        node_id.clone(),
        vec![address.clone()].into(),
        Default::default(),
        Default::default(),
        &[],
    ))
}
