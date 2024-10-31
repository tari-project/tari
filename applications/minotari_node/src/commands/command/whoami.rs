//  Copyright 2022, The Tari Project
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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use qrcode::{render::unicode, QrCode};
use tari_network::GlobalIp;

use super::{CommandContext, HandleCommand};

/// Display identity information about this node,
/// including: public key, node ID and the public address
#[derive(Debug, Parser)]
pub struct Args {
    /// Number of addresses to show
    #[clap(default_value_t = 5)]
    num_show_addrs: usize,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.whoami(args).await
    }
}

impl CommandContext {
    /// Function to process the whoami command
    pub async fn whoami(&self, args: Args) -> Result<(), Error> {
        let peer_info = self.network.get_local_peer_info().await?;

        let pk = peer_info.public_key.try_into_sr25519()?;
        let num_addrs = peer_info.listen_addrs.len();
        let (global, local) = peer_info
            .listen_addrs
            .iter()
            .partition::<Vec<_>, _>(|addr| addr.is_global_ip());
        let qr_addresses = global
            .iter()
            .chain(Some(&local).filter(|_| !global.is_empty()).into_iter().flatten())
            .take(args.num_show_addrs)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("::");

        let peer_str = format!("{}::{}", pk.inner_key(), qr_addresses);

        println!("🔑 Public Key:  {}", pk.inner_key());
        println!("🪪 Peer ID:  {}", peer_info.peer_id);
        println!("🏠️ Addresses ({num_addrs})");
        for addr in global.into_iter().chain(local).take(args.num_show_addrs) {
            println!("- {addr}");
        }
        if num_addrs > 0 && num_addrs > args.num_show_addrs {
            println!("{} more...", num_addrs - args.num_show_addrs);
        }

        println!();
        println!("{peer_str}");
        println!();
        let network = self.config.network();
        let qr_link = format!(
            "tari://{}/base_nodes/add?name={}&peer_str={}",
            network, peer_info.peer_id, peer_str
        );
        let code = QrCode::new(qr_link)?;
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .build()
            .lines()
            .skip(1)
            .fold("".to_string(), |acc, l| format!("{}{}\n", acc, l));
        println!("{}", image);
        Ok(())
    }
}
