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

use crate::{conversions::datetime_to_timestamp, tari_rpc as grpc};
use tari_comms::{net_address::MutliaddrWithStats, peer_manager::Peer};
use tari_crypto::tari_utilities::ByteArray;

impl From<Peer> for grpc::Peer {
    fn from(peer: Peer) -> Self {
        let public_key = peer.public_key.to_vec();
        let node_id = peer.node_id.to_vec();
        let mut addresses: Vec<grpc::Address> = Vec::new();
        let last_connection = match peer.addresses.last_seen() {
            Some(v) => Some(datetime_to_timestamp((v.timestamp() as u64).into())),
            None => Some(datetime_to_timestamp(0.into())),
        };
        for address in peer.addresses.addresses {
            addresses.push(address.clone().into())
        }
        let flags = peer.flags.bits() as u32;
        let banned_until = match peer.banned_until {
            Some(v) => Some(datetime_to_timestamp((v.timestamp() as u64).into())),
            None => Some(datetime_to_timestamp(0.into())),
        };
        let banned_reason = peer.banned_reason.to_string();
        let offline_at = match peer.offline_at {
            Some(v) => Some(datetime_to_timestamp((v.timestamp() as u64).into())),
            None => Some(datetime_to_timestamp(0.into())),
        };
        let features = peer.features.bits();

        let last_connected_at = match peer.connection_stats.last_connected_at {
            Some(v) => Some(datetime_to_timestamp((v.timestamp() as u64).into())),
            None => Some(datetime_to_timestamp(0.into())),
        };
        let supported_protocols = peer.supported_protocols.into_iter().map(|p| p.to_vec()).collect();
        let user_agent = peer.user_agent;
        Self {
            public_key,
            node_id,
            addresses,
            last_connection,
            flags,
            banned_until,
            banned_reason,
            offline_at,
            features,
            last_connected_at,
            supported_protocols,
            user_agent,
        }
    }
}

impl From<MutliaddrWithStats> for grpc::Address {
    fn from(address_with_stats: MutliaddrWithStats) -> Self {
        let address = address_with_stats.address.to_vec();
        let last_seen = match address_with_stats.last_seen {
            Some(v) => v.to_string(),
            None => "".to_string(),
        };
        let connection_attempts = address_with_stats.connection_attempts;
        let rejected_message_count = address_with_stats.rejected_message_count;
        let avg_latency = address_with_stats.avg_latency.as_secs();
        Self {
            address,
            last_seen,
            connection_attempts,
            rejected_message_count,
            avg_latency,
        }
    }
}
