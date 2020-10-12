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

use crate::{
    net_address::MultiaddressesWithStats,
    peer_manager::{
        connection_stats::PeerConnectionStats,
        migrations::Migration,
        node_id::deserialize_node_id_from_hex,
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerId,
    },
    protocol::ProtocolId,
    types::CommsPublicKey,
};
use chrono::NaiveDateTime;
use log::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tari_crypto::tari_utilities::hex::serialize_to_hex;
use tari_storage::{
    lmdb_store::{LMDBDatabase, LMDBError},
    IterationResult,
};

const LOG_TARGET: &str = "comms::peer_manager::migrations::v3";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerV3 {
    pub id: Option<PeerId>,
    pub public_key: CommsPublicKey,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    pub addresses: MultiaddressesWithStats,
    pub flags: PeerFlags,
    pub banned_until: Option<NaiveDateTime>,
    pub banned_reason: String,
    pub offline_at: Option<NaiveDateTime>,
    pub features: PeerFeatures,
    pub connection_stats: PeerConnectionStats,
    pub supported_protocols: Vec<ProtocolId>,
    pub added_at: NaiveDateTime,
    pub user_agent: String,
}
/// This migration is to the metadata field
pub struct MigrationV3;

impl Migration<LMDBDatabase> for MigrationV3 {
    type Error = LMDBError;

    fn migrate(&self, db: &LMDBDatabase) -> Result<(), Self::Error> {
        db.for_each::<PeerId, PeerV3, _>(|old_peer| {
            match old_peer {
                Ok((key, peer)) => {
                    debug!(target: LOG_TARGET, "Migrating peer `{}`", peer.node_id.short_str());
                    let result = db.insert(&key, &Peer {
                        id: peer.id,
                        public_key: peer.public_key,
                        node_id: peer.node_id,
                        addresses: peer.addresses,
                        flags: peer.flags,
                        banned_until: peer.banned_until,
                        banned_reason: "".to_string(),
                        offline_at: peer.offline_at,
                        features: peer.features,
                        connection_stats: peer.connection_stats,
                        supported_protocols: peer.supported_protocols,
                        added_at: peer.added_at,
                        user_agent: peer.user_agent,
                        metadata: HashMap::new(),
                    });

                    if let Err(err) = result {
                        error!(
                            target: LOG_TARGET,
                            "Failed to insert peer: {}. ** Database may be corrupt **", err
                        );
                    }
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to deserialize peer: {} ** Database may be corrupt **", err
                    );
                },
            }
            IterationResult::Continue
        })?;

        Ok(())
    }
}
