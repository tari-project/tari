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
        migrations::{v2::PeerV2, Migration},
        node_id::deserialize_node_id_from_hex,
        NodeId,
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
use tari_crypto::tari_utilities::hex::serialize_to_hex;
use tari_storage::{
    lmdb_store::{LMDBDatabase, LMDBError},
    IterationResult,
};

const LOG_TARGET: &str = "comms::peer_manager::migrations::v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerV1 {
    id: Option<PeerId>,
    public_key: CommsPublicKey,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    node_id: NodeId,
    addresses: MultiaddressesWithStats,
    flags: PeerFlags,
    banned_until: Option<NaiveDateTime>,
    offline_at: Option<NaiveDateTime>,
    features: PeerFeatures,
    connection_stats: PeerConnectionStats,
    supported_protocols: Vec<ProtocolId>,
    added_at: NaiveDateTime,
}

/// This migration is to add user_agent field
pub struct MigrationV1;

impl Migration<LMDBDatabase> for MigrationV1 {
    type Error = LMDBError;

    fn migrate(&self, db: &LMDBDatabase) -> Result<(), Self::Error> {
        db.for_each::<PeerId, PeerV1, _>(|old_peer| {
            match old_peer {
                Ok((key, peer)) => {
                    debug!(target: LOG_TARGET, "Migrating peer `{}`", peer.node_id.short_str());
                    let result = db.insert(&key, &PeerV2 {
                        id: peer.id,
                        public_key: peer.public_key,
                        node_id: peer.node_id,
                        addresses: peer.addresses,
                        flags: peer.flags,
                        banned_until: peer.banned_until,
                        offline_at: peer.offline_at,
                        features: peer.features,
                        connection_stats: peer.connection_stats,
                        supported_protocols: peer.supported_protocols,
                        added_at: peer.added_at,
                        user_agent: String::new(),
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
