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

use std::collections::HashMap;

use chrono::NaiveDateTime;
use log::*;
use serde::{Deserialize, Serialize};
use tari_crypto::tari_utilities::hex::serialize_to_hex;
use tari_storage::{
    lmdb_store::{LMDBDatabase, LMDBError},
    IterationResult,
};

use crate::{
    net_address::MultiaddressesWithStats,
    peer_manager::{
        connection_stats::PeerConnectionStats,
        migrations::MIGRATION_VERSION_KEY,
        node_id::deserialize_node_id_from_hex,
        IdentitySignature,
        NodeId,
        PeerFeatures,
        PeerFlags,
        PeerId,
    },
    protocol::ProtocolId,
    types::CommsPublicKey,
};

const LOG_TARGET: &str = "comms::peer_manager::migrations::v4";

#[derive(Debug, Deserialize, Serialize)]
pub struct PeerV4 {
    pub(super) id: Option<PeerId>,
    pub public_key: CommsPublicKey,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    pub addresses: MultiaddressesWithStats,
    pub flags: PeerFlags,
    pub banned_until: Option<NaiveDateTime>,
    pub banned_reason: String,
    pub offline_at: Option<NaiveDateTime>,
    pub last_seen: Option<NaiveDateTime>,
    pub features: PeerFeatures,
    pub connection_stats: PeerConnectionStats,
    pub supported_protocols: Vec<ProtocolId>,
    pub added_at: NaiveDateTime,
    pub user_agent: String,
    pub metadata: HashMap<u8, Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeerV5 {
    pub(super) id: Option<PeerId>,
    pub public_key: CommsPublicKey,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    pub addresses: MultiaddressesWithStats,
    pub flags: PeerFlags,
    pub banned_until: Option<NaiveDateTime>,
    pub banned_reason: String,
    pub offline_at: Option<NaiveDateTime>,
    pub last_seen: Option<NaiveDateTime>,
    pub features: PeerFeatures,
    pub connection_stats: PeerConnectionStats,
    pub supported_protocols: Vec<ProtocolId>,
    pub added_at: NaiveDateTime,
    pub user_agent: String,
    pub metadata: HashMap<u8, Vec<u8>>,
    pub identity_signature: Option<IdentitySignature>,
}

pub struct Migration;

impl super::Migration<LMDBDatabase> for Migration {
    type Error = LMDBError;

    fn get_version(&self) -> u32 {
        5
    }

    fn migrate(&self, db: &LMDBDatabase) -> Result<(), Self::Error> {
        db.for_each::<PeerId, PeerV4, _>(|old_peer| {
            let result = old_peer.and_then(|(key, peer)| {
                if key == MIGRATION_VERSION_KEY {
                    return Ok(());
                }

                debug!(target: LOG_TARGET, "Migrating peer `{}`", peer.node_id.short_str());
                db.insert(&key, &PeerV5 {
                    id: peer.id,
                    public_key: peer.public_key,
                    node_id: peer.node_id,
                    addresses: peer.addresses,
                    flags: peer.flags,
                    banned_until: peer.banned_until,
                    banned_reason: peer.banned_reason,
                    offline_at: peer.offline_at,
                    last_seen: peer.last_seen,
                    features: peer.features,
                    connection_stats: peer.connection_stats,
                    supported_protocols: peer.supported_protocols,
                    added_at: peer.added_at,
                    user_agent: peer.user_agent,
                    metadata: peer.metadata,
                    identity_signature: None,
                })
                .map_err(Into::into)
            });

            if let Err(err) = result {
                error!(
                    target: LOG_TARGET,
                    "Failed to deserialize peer: {} ** Database may be corrupt **", err
                );
            }
            IterationResult::Continue
        })?;

        Ok(())
    }
}
