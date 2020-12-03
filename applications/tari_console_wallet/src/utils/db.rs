// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use log::*;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
};
use tari_core::transactions::types::PublicKey;
use tari_crypto::tari_utilities::hex::Hex;
use tari_wallet::WalletSqlite;

pub const LOG_TARGET: &str = "wallet::utils::db";
pub const CUSTOM_BASE_NODE_PUBLIC_KEY_KEY: &str = "console_wallet_custom_base_node_public_key";
pub const CUSTOM_BASE_NODE_ADDRESS_KEY: &str = "console_wallet_custom_base_node_address";

/// This helper function will attempt to read a stored base node public key and address from the wallet database if
/// possible. If both are found they are used to construct and return a Peer.
pub async fn get_custom_base_node_peer_from_db(wallet: &mut WalletSqlite) -> Option<Peer> {
    let custom_base_node_peer_pubkey = match wallet
        .db
        .get_client_key_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string())
        .await
    {
        Ok(val) => val,
        Err(e) => {
            warn!(target: LOG_TARGET, "Problem reading from wallet database: {}", e);
            return None;
        },
    };
    let custom_base_node_peer_address = match wallet
        .db
        .get_client_key_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string())
        .await
    {
        Ok(val) => val,
        Err(e) => {
            warn!(target: LOG_TARGET, "Problem reading from wallet database: {}", e);
            return None;
        },
    };

    match (custom_base_node_peer_pubkey, custom_base_node_peer_address) {
        (Some(public_key), Some(address)) => {
            let pub_key_str = PublicKey::from_hex(public_key.as_str());
            let addr_str = address.parse::<Multiaddr>();
            let (pub_key, address) = match (pub_key_str, addr_str) {
                (Ok(pk), Ok(addr)) => (pk, addr),
                (_, _) => {
                    debug!(
                        target: LOG_TARGET,
                        "Problem converting stored custom base node public key or address"
                    );
                    return None;
                },
            };

            let node_id = match NodeId::from_key(&pub_key) {
                Ok(n) => n,
                Err(e) => {
                    debug!(
                        target: LOG_TARGET,
                        "Problem converting stored base node public key to Node Id: {}", e
                    );
                    return None;
                },
            };
            Some(Peer::new(
                pub_key,
                node_id,
                address.into(),
                PeerFlags::default(),
                PeerFeatures::COMMUNICATION_NODE,
                Default::default(),
                Default::default(),
            ))
        },
        (_, _) => None,
    }
}
