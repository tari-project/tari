// Copyright 2020, The Tari Project
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

use std::{cmp::min, collections::HashMap, str::FromStr, time::Duration};

use bytes::Bytes;
use chrono::{NaiveDateTime, TimeDelta};
use diesel::{
    self,
    dsl::now,
    prelude::*,
    r2d2::{ConnectionManager, PooledConnection},
    ExpressionMethods,
    QueryDsl,
    RunQueryDsl,
    SqliteConnection,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use log::{trace, warn};
use multiaddr::Multiaddr;
use tari_common_sqlite::{connection::DbConnection, error::StorageError};
use tari_utilities::{hex, hex::Hex};

use crate::{
    net_address::{MultiaddrWithStats, MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        generate_peer_id_as_i64,
        peer_id::peer_id_from_i64,
        storage::schema::{multi_addresses, node_identity, peers},
        NodeDistance,
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerId,
    },
    protocol::ProtocolId,
    types::CommsPublicKey,
    utils::datetime::safe_future_datetime_from_duration,
};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/peer_manager/storage/migrations");
const LOG_TARGET: &str = "comms::peer_manager::storage::db";

/// This peer's identity information
#[derive(Clone)]
pub struct ThisPeerIdentity {
    pub public_key: CommsPublicKey,
    pub node_id: NodeId,
    pub features: PeerFeatures,
}

/// Peers database containing peers data
#[derive(Clone)]
pub struct PeerDatabaseSql {
    connection: DbConnection,
    this_peer_identity: ThisPeerIdentity,
}

impl PeerDatabaseSql {
    /// Create a new peers database using the provided connection
    pub fn new(connection: DbConnection, this_peer: &Peer) -> Result<Self, StorageError> {
        let instance = Self {
            connection,
            this_peer_identity: ThisPeerIdentity {
                public_key: this_peer.public_key.clone(),
                node_id: this_peer.node_id.clone(),
                features: this_peer.features,
            },
        };
        PeerDatabaseSql::add_this_peer_node_identity_to_db(&instance)?;
        Ok(instance)
    }

    /// Get this peer's identity
    pub fn this_peer_identity(&self) -> ThisPeerIdentity {
        self.this_peer_identity.clone()
    }

    fn add_this_peer_node_identity_to_db(&self) -> Result<(), StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let node_identity_indexes = node_identity::table.load::<NewThisPeerIdentitySql>(conn)?;
            if node_identity_indexes.len() > 1 {
                return Err(StorageError::UnexpectedResult(format!(
                    "There are multiple node identities for this peer in the database, expected 1, found {}",
                    node_identity_indexes.len()
                )));
            }
            if !node_identity_indexes.is_empty() {
                if self.this_peer_identity.public_key.to_hex() == node_identity_indexes[0].public_key &&
                    self.this_peer_identity.node_id.to_hex() == node_identity_indexes[0].node_id
                {
                    return Ok(());
                } else {
                    return Err(StorageError::UnexpectedResult(format!(
                        "This peer node identity does not match, expected '{}', found '{}'",
                        self.this_peer_identity.node_id.to_hex(),
                        node_identity_indexes[0].node_id
                    )));
                }
            }

            let node_identity_sql = NewThisPeerIdentitySql {
                public_key: self.this_peer_identity.public_key.to_hex(),
                node_id: self.this_peer_identity.node_id.to_hex(),
                features: self.this_peer_identity.features.to_i32(),
            };

            let inserted = diesel::insert_into(node_identity::table)
                .values(node_identity_sql)
                .execute(conn)?;
            if inserted == 0 {
                return Err(StorageError::UnexpectedResult(format!(
                    "Could not insert own node identity '{}'",
                    self.this_peer_identity.node_id
                )));
            }

            Ok(())
        })
    }

    // Note: This function is not properly working at the moment, but must be kept here for in its commented out form
    // for further evaluation.
    // ==============================================================================================================
    // /// This function will add peers and their associated multi-addresses in batch mode:
    // /// - New peers are added with all their information.
    // /// - Existing peers are not modified.
    // /// - Only missing multi-addresses are added for existing peers.
    // ///
    // ///   Note:
    // ///   SQLite does not support the DEFAULT keyword in INSERT statements, which Diesel uses for batch inserts.
    // ///   Diesel's batch insert API is designed for databases like PostgreSQL that support this feature.
    // #[allow(clippy::too_many_lines)]
    // pub fn batch_add_peers_with_addresses(
    //     &self,
    //     peers_with_addresses: Vec<NewPeerWithAddressesSql>,
    // ) -> Result<usize, StorageError> {
    //     let mut conn = self.connection.get_pooled_connection()?;
    //     conn.immediate_transaction::<_, StorageError, _>(|conn| {
    //         // Step 1: Insert new peers with ON CONFLICT DO NOTHING
    //         let values = peers_with_addresses
    //             .iter()
    //             .map(|p| {
    //                 let peer_id = generate_peer_id_as_i64();
    //                 let public_key = sql_escape(&p.peer.public_key);
    //                 let node_id = sql_escape(&p.peer.node_id);
    //                 let distance_to_self = self
    //                     .this_peer_identity
    //                     .node_id
    //                     .distance(&NodeId::from_hex(&p.peer.node_id)?)
    //                     .to_string();
    //                 let flags = p.peer.flags;
    //                 let banned_until = p.peer.banned_until.map_or("NULL".to_string(), |dt| format!("'{}'", dt));
    //                 let banned_reason = p
    //                     .peer
    //                     .banned_reason
    //                     .clone()
    //                     .map_or("NULL".to_string(), |reason| format!("'{}'", sql_escape(&reason)));
    //                 let features = p.peer.features;
    //                 let supported_protocols = sql_escape(&p.peer.supported_protocols);
    //                 let added_at = p.peer.added_at;
    //                 let user_agent = sql_escape(&p.peer.user_agent);
    //                 let metadata = p
    //                     .peer
    //                     .metadata
    //                     .clone()
    //                     .map_or("NULL".to_string(), |meta| format!("x'{}'", hex::to_hex(&meta)));
    //                 let deleted_at = p.peer.deleted_at.map_or("NULL".to_string(), |dt| format!("'{}'", dt));
    //
    //                 Ok::<String, StorageError>(format!(
    //                     "({}, '{}', '{}', '{}', {}, {}, {}, {}, '{}', '{}', '{}', {}, {})",
    //                     peer_id,
    //                     public_key,
    //                     node_id,
    //                     distance_to_self,
    //                     flags,
    //                     banned_until,
    //                     banned_reason,
    //                     features,
    //                     supported_protocols,
    //                     added_at,
    //                     user_agent,
    //                     metadata,
    //                     deleted_at
    //                 ))
    //             })
    //             .collect::<Result<Vec<String>, _>>()?;
    //
    //         let mut peer_query = format!(
    //             "INSERT INTO peers (peer_id, public_key, node_id, distance_to_self, flags, banned_until, \
    //              banned_reason, features, supported_protocols, added_at, user_agent, metadata, deleted_at) VALUES
    // {}",             values.join(", ")
    //         );
    //
    //         peer_query.push_str(" ON CONFLICT (node_id) DO NOTHING");
    //         conn.batch_execute(&peer_query)?;
    //
    //         // Step 2: Collect all multi-addresses into a map
    //         let mut address_map: HashMap<String, Vec<NewMultiaddrWithStatsSql>> = HashMap::new();
    //         for item in peers_with_addresses {
    //             address_map
    //                 .entry(item.peer.node_id.to_string())
    //                 .or_default()
    //                 .extend(item.addresses);
    //         }
    //
    //         // Step 3: Insert missing multi-addresses
    //         let mut address_query = String::from(
    //             "INSERT INTO multi_addresses (peer_id, address, last_seen, connection_attempts, \
    //              avg_initial_dial_time, initial_dial_time_sample_count, avg_latency, latency_sample_count, \
    //              last_attempted, last_failed_reason, quality_score, source) VALUES ",
    //         );
    //
    //         let mut total_addresses_inserted = 0;
    //         for (node_id, addresses) in address_map {
    //             // Retrieve peer_id for the node_id
    //             let peer_id = peers::table
    //                 .filter(peers::node_id.eq(node_id))
    //                 .select(peers::peer_id)
    //                 .first::<i64>(conn)?;
    //
    //             // Filter out existing addresses
    //             let existing_addresses: Vec<_> = multi_addresses::table
    //                 .filter(multi_addresses::peer_id.eq(peer_id))
    //                 .select(multi_addresses::address)
    //                 .load::<String>(conn)?;
    //
    //             let new_addresses: Vec<_> = addresses
    //                 .into_iter()
    //                 .filter(|addr| !existing_addresses.contains(&addr.address.to_string()))
    //                 .collect();
    //
    //             if !new_addresses.is_empty() {
    //                 address_query.push_str(
    //                     &new_addresses
    //                         .iter()
    //                         .map(|addr| {
    //                             format!(
    //                                 "({}, '{}', {}, {}, {}, {}, {}, {}, {}, {}, {}, '{}')",
    //                                 peer_id,
    //                                 sql_escape(&addr.address),
    //                                 addr.last_seen.map_or("NULL".to_string(), |dt| format!("'{}'", dt)),
    //                                 addr.connection_attempts.map_or("NULL".to_string(), |v| v.to_string()),
    //                                 addr.avg_initial_dial_time.map_or("NULL".to_string(), |v| v.to_string()),
    //                                 addr.initial_dial_time_sample_count
    //                                     .map_or("NULL".to_string(), |v| v.to_string()),
    //                                 addr.avg_latency.map_or("NULL".to_string(), |v| v.to_string()),
    //                                 addr.latency_sample_count.map_or("NULL".to_string(), |v| v.to_string()),
    //                                 addr.last_attempted.map_or("NULL".to_string(), |dt| format!("'{}'", dt)),
    //                                 addr.last_failed_reason
    //                                     .clone()
    //                                     .map_or("NULL".to_string(), |reason| format!("'{}'", sql_escape(&reason))),
    //                                 addr.quality_score.map_or("NULL".to_string(), |v| v.to_string()),
    //                                 sql_escape(&addr.source),
    //                             )
    //                         })
    //                         .collect::<Vec<String>>()
    //                         .join(", "),
    //                 );
    //
    //                 total_addresses_inserted += new_addresses.len();
    //             }
    //         }
    //
    //         if total_addresses_inserted > 0 {
    //             address_query.push_str(" ON CONFLICT (address) DO NOTHING");
    //             conn.batch_execute(&address_query)?;
    //         }
    //
    //         Ok(total_addresses_inserted)
    //     })
    // }
    // ==============================================================================================================

    // Note: This function is not properly working at the moment, but must be kept here for in its commented out form
    // for further evaluation.
    // ==============================================================================================================
    // /// This function will update peers and their associated multi-addresses in batch mode.
    // #[allow(clippy::too_many_lines)]
    // pub fn batch_update_peers_with_addresses(
    //     &self,
    //     peers_with_addresses: Vec<UpdatePeerWithAddressesSql>,
    // ) -> Result<(), StorageError> {
    //     let mut conn = self.connection.get_pooled_connection()?;
    //     conn.immediate_transaction::<_, StorageError, _>(|conn| {
    //         // Batch update peers
    //         if !peers_with_addresses.is_empty() {
    //             let mut peer_query = String::from("UPDATE peers SET ");
    //             let mut set_clauses = vec![];
    //             let mut node_ids = vec![];
    //
    //             for update in &peers_with_addresses {
    //                 let peer_update = update.peer.clone();
    //
    //                 if let Some(banned_until) = peer_update.banned_until {
    //                     set_clauses.push(format!(
    //                         "banned_until = CASE WHEN node_id = '{}' THEN '{}' ELSE banned_until END",
    //                         peer_update.node_id, banned_until
    //                     ));
    //                 }
    //                 if let Some(banned_reason) = peer_update.banned_reason {
    //                     set_clauses.push(format!(
    //                         "banned_reason = CASE WHEN node_id = '{}' THEN '{}' ELSE banned_reason END",
    //                         peer_update.node_id,
    //                         sql_escape(&banned_reason)
    //                     ));
    //                 }
    //                 if let Some(supported_protocols) = peer_update.supported_protocols {
    //                     set_clauses.push(format!(
    //                         "supported_protocols = CASE WHEN node_id = '{}' THEN '{}' ELSE supported_protocols END",
    //                         peer_update.node_id,
    //                         sql_escape(&supported_protocols)
    //                     ));
    //                 }
    //                 if let Some(user_agent) = peer_update.user_agent {
    //                     set_clauses.push(format!(
    //                         "user_agent = CASE WHEN node_id = '{}' THEN '{}' ELSE user_agent END",
    //                         peer_update.node_id,
    //                         sql_escape(&user_agent)
    //                     ));
    //                 }
    //                 if let Some(metadata) = peer_update.metadata {
    //                     set_clauses.push(format!(
    //                         "metadata = CASE WHEN node_id = '{}' THEN x'{}' ELSE metadata END",
    //                         peer_update.node_id,
    //                         hex::to_hex(&metadata)
    //                     ));
    //                 }
    //                 if let Some(deleted_at) = peer_update.deleted_at {
    //                     set_clauses.push(format!(
    //                         "deleted_at = CASE WHEN node_id = '{}' THEN '{}' ELSE deleted_at END",
    //                         peer_update.node_id, deleted_at
    //                     ));
    //                 }
    //                 node_ids.push(format!("'{}'", peer_update.node_id.replace('\'', "''")));
    //             }
    //
    //             peer_query.push_str(&set_clauses.join(", "));
    //             peer_query.push_str(&format!(" WHERE node_id IN ({})", node_ids.join(", ")));
    //             conn.batch_execute(&peer_query)?;
    //         }
    //
    //         // Batch update multi-addresses
    //         let mut address_query = String::from("UPDATE multi_addresses SET ");
    //         let mut set_clauses = vec![];
    //         let mut addresses = vec![];
    //
    //         for update in peers_with_addresses {
    //             for address_update in update.addresses {
    //                 if let Some(last_seen) = address_update.last_seen {
    //                     set_clauses.push(format!(
    //                         "last_seen = CASE WHEN address = '{}' THEN '{}' ELSE last_seen END",
    //                         address_update.address, last_seen
    //                     ));
    //                 }
    //                 if let Some(connection_attempts) = address_update.connection_attempts {
    //                     set_clauses.push(format!(
    //                         "connection_attempts = CASE WHEN address = '{}' THEN {} ELSE connection_attempts END",
    //                         address_update.address, connection_attempts
    //                     ));
    //                 }
    //                 if let Some(avg_initial_dial_time) = address_update.avg_initial_dial_time {
    //                     set_clauses.push(format!(
    //                         "avg_initial_dial_time = CASE WHEN address = '{}' THEN {} ELSE avg_initial_dial_time
    // END",                         address_update.address, avg_initial_dial_time
    //                     ));
    //                 }
    //                 if let Some(initial_dial_time_sample_count) = address_update.initial_dial_time_sample_count {
    //                     set_clauses.push(format!(
    //                         "initial_dial_time_sample_count = CASE WHEN address = '{}' THEN {} ELSE \
    //                          initial_dial_time_sample_count END",
    //                         address_update.address, initial_dial_time_sample_count
    //                     ));
    //                 }
    //                 if let Some(avg_latency) = address_update.avg_latency {
    //                     set_clauses.push(format!(
    //                         "avg_latency = CASE WHEN address = '{}' THEN {} ELSE avg_latency END",
    //                         address_update.address, avg_latency
    //                     ));
    //                 }
    //                 if let Some(latency_sample_count) = address_update.latency_sample_count {
    //                     set_clauses.push(format!(
    //                         "latency_sample_count = CASE WHEN address = '{}' THEN {} ELSE latency_sample_count END",
    //                         address_update.address, latency_sample_count
    //                     ));
    //                 }
    //                 if let Some(last_attempted) = address_update.last_attempted {
    //                     set_clauses.push(format!(
    //                         "last_attempted = CASE WHEN address = '{}' THEN '{}' ELSE last_attempted END",
    //                         address_update.address, last_attempted
    //                     ));
    //                 }
    //                 if let Some(last_failed_reason) = address_update.last_failed_reason {
    //                     set_clauses.push(format!(
    //                         "last_failed_reason = CASE WHEN address = '{}' THEN '{}' ELSE last_failed_reason END",
    //                         address_update.address,
    //                         sql_escape(&last_failed_reason)
    //                     ));
    //                 }
    //                 if let Some(quality_score) = address_update.quality_score {
    //                     set_clauses.push(format!(
    //                         "quality_score = CASE WHEN address = '{}' THEN {} ELSE quality_score END",
    //                         address_update.address, quality_score
    //                     ));
    //                 }
    //                 if let Some(source) = address_update.source {
    //                     set_clauses.push(format!(
    //                         "source = CASE WHEN address = '{}' THEN '{}' ELSE source END",
    //                         address_update.address,
    //                         sql_escape(&source)
    //                     ));
    //                 }
    //                 addresses.push(format!("'{}'", address_update.address.replace('\'', "''")));
    //             }
    //         }
    //
    //         if !set_clauses.is_empty() {
    //             address_query.push_str(&set_clauses.join(", "));
    //             address_query.push_str(&format!(" WHERE address IN ({})", addresses.join(", ")));
    //             conn.batch_execute(&address_query)?;
    //         }
    //
    //         Ok(())
    //     })
    // }
    // ==============================================================================================================

    /// Add a new peer or update an existing peer with its associated multi-addresses
    pub fn add_or_update_peer(&self, peer: Peer) -> Result<PeerId, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let node_id = peer.node_id.clone();

            match self.get_peer_by_node_id_inner(&node_id, conn)? {
                Some(mut existing_peer) => {
                    trace!(target: LOG_TARGET, "Replacing peer that has NodeId '{}'", node_id);
                    existing_peer.merge(&peer);
                    let update_peer_sql = PeerDatabaseSql::update_peer_sql(existing_peer.clone())?;
                    self.update_peer_inner(update_peer_sql, conn)?;
                    Ok(existing_peer.id.unwrap_or_default())
                },
                None => {
                    trace!(target: LOG_TARGET, "Adding peer with node id '{}'", node_id);
                    let new_peer_sql = self.add_peer_sql(peer)?;
                    let peer_id = self.add_peer_inner(new_peer_sql, conn)?;
                    Ok(peer_id)
                },
            }
        })
    }

    /// Adds or updates a peer and sets the last connection as successful.
    /// If the peer is marked as offline, it will be unmarked.
    pub fn add_or_update_online_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: &NodeId,
        addresses: &[Multiaddr],
        peer_features: &PeerFeatures,
        source: &PeerAddressSource,
    ) -> Result<Peer, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            match self.get_peer_by_node_id_inner(node_id, conn)? {
                Some(mut peer) => {
                    // Update existing
                    peer.addresses.update_addresses(addresses, source);
                    peer.features = *peer_features;
                    let update_peer_sql = PeerDatabaseSql::update_peer_sql(peer.clone())?;
                    self.update_peer_inner(update_peer_sql, conn)?;
                    Ok(peer)
                },
                None => {
                    // Create new
                    let new_peer = Peer::new(
                        pubkey.clone(),
                        node_id.clone(),
                        MultiaddressesWithStats::from_addresses_with_source(addresses.to_vec(), source),
                        PeerFlags::default(),
                        *peer_features,
                        Default::default(),
                        Default::default(),
                    );
                    let new_peer_sql = self.add_peer_sql(new_peer.clone())?;
                    let peer_id = self.add_peer_inner(new_peer_sql, conn)?;
                    let mut peer = new_peer;
                    peer.set_id(peer_id);
                    Ok(peer)
                },
            }
        })
    }

    // Helper function to convert a Peer to a NewPeerWithAddressesSql
    fn add_peer_sql(&self, peer: Peer) -> Result<NewPeerWithAddressesSql, StorageError> {
        let new_peer_sql = NewPeerSql {
            peer_id: generate_peer_id_as_i64(),
            public_key: peer.public_key.to_hex(),
            node_id: peer.node_id.to_hex(),
            distance_to_self: format!(
                "{:032}",
                self.this_peer_identity.node_id.distance(&peer.node_id).as_u128()
            ),
            flags: peer.flags.to_i32(),
            banned_until: peer.banned_until,
            banned_reason: Some(peer.banned_reason.clone()),
            features: peer.features.to_i32(),
            supported_protocols: serialize_protocols(&peer.supported_protocols),
            added_at: peer.added_at,
            user_agent: peer.user_agent.clone(),
            metadata: serialize_metadata(&peer.metadata)?,
            deleted_at: peer.deleted_at,
        };

        let mut new_addresses_sql = Vec::with_capacity(peer.addresses.len());
        for address in peer.addresses.iter() {
            new_addresses_sql.push(NewMultiaddrWithStatsSql {
                address_id: None, // This will be set automatically
                peer_id: 0,       // This will be set automatically
                address: address.address().to_string(),
                last_seen: address.last_seen(),
                connection_attempts: if address.connection_attempts() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.connection_attempts())?)
                },
                avg_initial_dial_time: duration_to_i64_ms_infallible(address.avg_initial_dial_time()),
                initial_dial_time_sample_count: if address.initial_dial_time_sample_count() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.initial_dial_time_sample_count())?)
                },
                avg_latency: duration_to_i64_ms_infallible(address.avg_latency()),
                latency_sample_count: if address.latency_sample_count() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.latency_sample_count())?)
                },
                last_attempted: address.last_attempted(),
                last_failed_reason: address.last_failed_reason().map(|s| s.to_string()),
                quality_score: address.quality_score(),
                source: serde_json::to_string(&address.source())
                    .map_err(|err| StorageError::UnexpectedResult(err.to_string()))?,
            });
        }

        let new_peer_sql = NewPeerWithAddressesSql {
            peer: new_peer_sql,
            addresses: new_addresses_sql,
        };

        Ok(new_peer_sql)
    }

    // Add a new peer with its associated multi-addresses
    fn add_peer_inner(
        &self,
        new_peer_sql: NewPeerWithAddressesSql,
        conn: &mut SqliteConnection,
    ) -> Result<PeerId, StorageError> {
        // Insert the peer and get the last inserted ID
        let node_id = new_peer_sql.peer.node_id.clone();
        let inserted = diesel::insert_into(peers::table)
            .values(&new_peer_sql.peer)
            .execute(conn)?;
        if inserted == 0 {
            return Err(StorageError::UnexpectedResult(format!(
                "Could not insert peer '{}'",
                node_id
            )));
        }

        let peer_id = peers::table
            .filter(peers::node_id.eq(new_peer_sql.peer.node_id))
            .select(peers::peer_id)
            .first::<i64>(conn)?;

        // Batch insert the associated multi-addresses
        let addresses: Vec<_> = new_peer_sql
            .addresses
            .clone()
            .iter_mut()
            .map(|addr| {
                addr.peer_id = peer_id;
                addr.clone()
            })
            .collect();

        let inserted = diesel::insert_into(multi_addresses::table)
            .values(&addresses)
            .execute(conn)?;
        if inserted != addresses.len() {
            return Err(StorageError::UnexpectedResult(format!(
                "Could not insert address '{:?}' for peer '{}'",
                new_peer_sql
                    .addresses
                    .iter()
                    .map(|v| v.address.clone())
                    .collect::<Vec<_>>(),
                node_id
            )));
        }

        Ok(peer_id_from_i64(peer_id))
    }

    // Helper function to convert a Peer to an UpdatePeerWithAddressesSql
    fn update_peer_sql(peer: Peer) -> Result<UpdatePeerWithAddressesSql, StorageError> {
        let update_peer_sql = UpdatePeerSql {
            node_id: peer.node_id.to_hex(),
            banned_until: peer.banned_until,
            banned_reason: Some(peer.banned_reason.clone()),
            supported_protocols: Some(serialize_protocols(&peer.supported_protocols)),
            user_agent: Some(peer.user_agent.clone()),
            metadata: serialize_metadata(&peer.metadata)?,
            deleted_at: peer.deleted_at,
        };

        let mut update_addresses_sql = Vec::with_capacity(peer.addresses.len());
        for address in peer.addresses.iter() {
            update_addresses_sql.push(UpdateMultiaddrWithStatsSql {
                address: address.address().to_string(),
                last_seen: address.last_seen(),
                connection_attempts: if address.connection_attempts() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.connection_attempts())?)
                },
                avg_initial_dial_time: duration_to_i64_ms_infallible(address.avg_initial_dial_time()),
                initial_dial_time_sample_count: if address.initial_dial_time_sample_count() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.initial_dial_time_sample_count())?)
                },
                avg_latency: duration_to_i64_ms_infallible(address.avg_latency()),
                latency_sample_count: if address.latency_sample_count() == 0 {
                    None
                } else {
                    Some(i32::try_from(address.latency_sample_count())?)
                },
                last_attempted: address.last_attempted(),
                last_failed_reason: address.last_failed_reason().map(|s| s.to_string()),
                quality_score: address.quality_score(),
                source: Some(
                    serde_json::to_string(&address.source())
                        .map_err(|err| StorageError::UnexpectedResult(err.to_string()))?,
                ),
            });
        }

        let update_peer_sql = UpdatePeerWithAddressesSql {
            peer: update_peer_sql,
            addresses: update_addresses_sql,
        };

        Ok(update_peer_sql)
    }

    // Update an existing peer with its associated multi-addresses
    fn update_peer_inner(
        &self,
        update_peer_sql: UpdatePeerWithAddressesSql,
        conn: &mut SqliteConnection,
    ) -> Result<(), StorageError> {
        // Update the peer
        diesel::update(peers::table.filter(peers::node_id.eq(update_peer_sql.peer.node_id.clone())))
            .set(&update_peer_sql.peer)
            .execute(conn)?;

        // Update the associated multi-addresses
        for address_update in update_peer_sql.addresses {
            let updated = diesel::update(
                multi_addresses::table.filter(multi_addresses::address.eq(address_update.address.clone())),
            )
            .set(&address_update)
            .execute(conn)?;
            // If the address does not exist, add it
            if updated == 0 {
                let peer_id = peers::table
                    .filter(peers::node_id.eq(update_peer_sql.peer.node_id.clone()))
                    .select(peers::peer_id)
                    .first::<i64>(conn)?;
                let new_address_sql = NewMultiaddrWithStatsSql::from((address_update.clone(), peer_id));
                diesel::insert_into(multi_addresses::table)
                    .values(&new_address_sql)
                    .execute(conn)?;
            }
        }

        Ok(())
    }

    /// Check if a peer exists by querying its node ID - if it exits the peer_id will be returned
    pub fn peer_exists_by_node_id(&self, node_id: &NodeId) -> Result<Option<PeerId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        if let Ok(peer_id) = peers::table
            .filter(peers::node_id.eq(node_id.to_hex()))
            .select(peers::peer_id)
            .first::<i64>(&mut conn)
        {
            Ok(Some(peer_id_from_i64(peer_id)))
        } else {
            Ok(None)
        }
    }

    /// Check if a peer exists by querying its public key - if it exits the peer_id will be returned
    pub fn peer_exists_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<PeerId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        if let Ok(peer_id) = peers::table
            .filter(peers::public_key.eq(public_key.to_hex()))
            .select(peers::peer_id)
            .first::<i64>(&mut conn)
        {
            Ok(Some(peer_id_from_i64(peer_id)))
        } else {
            Ok(None)
        }
    }

    /// Set the deleted_at timestamp for a peer
    pub fn set_deleted_at(&self, node_id: &NodeId) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set(peers::deleted_at.eq(chrono::Utc::now().naive_utc()))
                .execute(conn)?;
            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Set the metadata for a peer, returning 'None' if the value was empty and the old value if the value was updated
    pub fn set_metadata(&self, node_id: &NodeId, key: u8, data: Vec<u8>) -> Result<Option<Vec<u8>>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let metadata = peers::table
                .filter(peers::node_id.eq(node_id.to_string()))
                .select(peers::metadata)
                .first::<Option<Vec<u8>>>(conn)?;

            let mut metadata_hashmap = deserialize_metadata(metadata)?;
            let result = metadata_hashmap.insert(key, data);
            let metadata = serialize_metadata(&metadata_hashmap)?;

            diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set(peers::metadata.eq(metadata))
                .execute(conn)?;

            Ok(result)
        })
    }

    /// Set the banned metadata for a peer, returning 'Some(node_id)' if successful, 'None' otherwise
    pub fn set_banned(
        &self,
        node_id: &NodeId,
        ban_duration: Duration,
        banned_reason: String,
    ) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let dt = safe_future_datetime_from_duration(ban_duration);
            let banned_until = dt.naive_utc();

            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set((
                    peers::banned_until.eq(banned_until),
                    peers::banned_reason.eq(banned_reason),
                ))
                .execute(conn)?;
            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Reset the banned metadata for a peer, returning 'Some(node_id)' if successful, 'None' otherwise
    pub fn reset_banned(&self, node_id: &NodeId) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set((
                    peers::banned_until.eq(None::<NaiveDateTime>),
                    peers::banned_reason.eq(None::<String>),
                ))
                .execute(conn)?;
            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Reset all banned metadata for all peers that were banned
    pub fn reset_all_banned(&self) -> Result<usize, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(peers::table.filter(peers::banned_until.is_not_null()))
                .set((
                    peers::banned_until.eq(None::<NaiveDateTime>),
                    peers::banned_reason.eq(None::<String>),
                ))
                .execute(conn)?;
            Ok(affected)
        })
    }

    /// Reset all offline non-wallet peers (zero their connection attempts)
    pub fn reset_offline_non_wallet_peers(&self) -> Result<usize, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(multi_addresses::table)
                .filter(
                    multi_addresses::peer_id.eq_any(peers::table.select(peers::peer_id).filter(diesel::dsl::sql::<
                        diesel::sql_types::Bool,
                    >(
                        &format!("features & {} != 0", PeerFeatures::COMMUNICATION_NODE.to_i32()),
                    ))),
                )
                .filter(multi_addresses::connection_attempts.ne(0))
                .set((
                    multi_addresses::connection_attempts.eq(0),
                    multi_addresses::last_attempted.eq(None::<NaiveDateTime>),
                    multi_addresses::last_failed_reason.eq(None::<String>),
                ))
                .execute(conn)?;

            Ok(affected)
        })
    }

    /// Set the last seen metadata for a peer's address, returning 'Some(node_id)' if successful, 'None' otherwise
    pub fn set_last_seen(
        &self,
        node_id: &NodeId,
        last_seen: NaiveDateTime,
        address: &Multiaddr,
    ) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(
                multi_addresses::table
                    .filter(
                        multi_addresses::peer_id.nullable().eq(peers::table
                            .filter(peers::node_id.eq(node_id.to_string()))
                            .select(peers::peer_id)
                            .single_value()),
                    )
                    .filter(multi_addresses::address.eq(address.to_string())),
            )
            .set(multi_addresses::last_seen.eq(last_seen))
            .execute(conn)?;

            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Reset the last seen metadata for a peer's address, returning 'Some(node_id)' if successful, 'None' otherwise
    pub fn reset_last_seen(&self, node_id: &NodeId, address: &Multiaddr) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(
                multi_addresses::table
                    .filter(
                        multi_addresses::peer_id.nullable().eq(peers::table
                            .filter(peers::node_id.eq(node_id.to_string()))
                            .select(peers::peer_id)
                            .single_value()),
                    )
                    .filter(multi_addresses::address.eq(address.to_string())),
            )
            .set(multi_addresses::last_seen.eq(None::<NaiveDateTime>))
            .execute(conn)?;

            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Set the last failed reason metadata for a peer's address
    pub fn set_last_failed_reason(
        &self,
        node_id: &NodeId,
        last_failed_reason: String,
        address: &Multiaddr,
    ) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(
                multi_addresses::table
                    .filter(
                        multi_addresses::peer_id.nullable().eq(peers::table
                            .filter(peers::node_id.eq(node_id.to_string()))
                            .select(peers::peer_id)
                            .single_value()),
                    )
                    .filter(multi_addresses::address.eq(address.to_string())),
            )
            .set(multi_addresses::last_failed_reason.eq(sql_escape(&last_failed_reason)))
            .execute(conn)?;

            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Reset the last failed reason metadata for a peer's address
    pub fn reset_last_failed_reason(
        &self,
        node_id: &NodeId,
        address: &Multiaddr,
    ) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(
                multi_addresses::table
                    .filter(
                        multi_addresses::peer_id.nullable().eq(peers::table
                            .filter(peers::node_id.eq(node_id.to_string()))
                            .select(peers::peer_id)
                            .single_value()),
                    )
                    .filter(multi_addresses::address.eq(address.to_string())),
            )
            .set(multi_addresses::last_failed_reason.eq(None::<String>))
            .execute(conn)?;

            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    // Helper function to convert a Vec of join query results into a Vec of Peer
    fn peers_from_join_query(results: Vec<(NewPeerSql, NewMultiaddrWithStatsSql)>) -> Result<Vec<Peer>, StorageError> {
        if results.is_empty() {
            return Ok(Vec::new());
        }

        let mut peer_map: HashMap<i64, (NewPeerSql, Vec<NewMultiaddrWithStatsSql>)> =
            HashMap::with_capacity(results.len());
        for (peer, address) in results {
            let peer_id = peer.peer_id;
            peer_map
                .entry(peer_id)
                .or_insert_with(|| (peer, Vec::new()))
                .1
                .push(address);
        }

        let peers = peer_map
            .into_iter()
            .map(|(_, (peer, addresses))| Peer::try_from((peer, addresses)))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(peers)
    }

    /// Find all peers that match a partial node ID or public key
    pub fn find_all_peers_match_partial_key(&self, start_bytes: &[u8]) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        if start_bytes.is_empty() {
            return Ok(Vec::new());
        }
        let partial_key = hex::to_hex(start_bytes);

        if start_bytes.len() > CommsPublicKey::key_length() {
            return Err(StorageError::MessageFormatError(format!(
                "Invalid length ({}) for peer NodeId or PublicKey, must be less than or equal to {}",
                start_bytes.len(),
                CommsPublicKey::key_length(),
            )));
        }

        let mut results;
        if start_bytes.len() > NodeId::byte_size() {
            results = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::public_key.like(format!("{}%", partial_key)))
                .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;
        } else {
            results = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::node_id.like(format!("{}%", partial_key)))
                .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;

            if results.is_empty() {
                results = peers::table
                    .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                    .filter(peers::public_key.like(format!("{}%", partial_key)))
                    .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;
            }
        }

        PeerDatabaseSql::peers_from_join_query(results)
    }

    /// Return all peers in the database
    pub fn get_all_peers(&self, features: Option<PeerFeatures>) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| self.get_all_peers_inner(features, conn))
    }

    fn get_all_peers_inner(
        &self,
        features: Option<PeerFeatures>,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<Peer>, StorageError> {
        let mut query = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .into_boxed(); // Enables dynamic query building

        if let Some(features) = features {
            if features == PeerFeatures::COMMUNICATION_CLIENT {
                query = query.filter(peers::features.eq(features.to_i32()));
            } else {
                query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                    "features & {} != 0",
                    features.to_i32()
                )));
            }
        }

        let results = query.load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(conn)?;

        PeerDatabaseSql::peers_from_join_query(results)
    }

    // // Return all deleted peers' node_ids
    fn get_all_deleted_peers(
        &self,
        features: Option<PeerFeatures>,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<NodeId>, StorageError> {
        let mut query = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::deleted_at.is_not_null())
            .into_boxed(); // Enables dynamic query building

        if let Some(features) = features {
            if features == PeerFeatures::COMMUNICATION_CLIENT {
                query = query.filter(peers::features.eq(features.to_i32()));
            } else {
                query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                    "features & {} != 0",
                    features.to_i32()
                )));
            }
        }

        let peers = query.select(peers::node_id).load::<String>(conn)?;
        let peers = peers
            .into_iter()
            .map(|p| NodeId::from_hex(&p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(peers)
    }

    /// Return at most `n` peers from the database that are not banned and not deleted
    pub fn get_n_not_banned_or_deleted_peers(&self, number: usize) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::banned_until.is_null())
            .filter(peers::deleted_at.is_null())
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;

        let peers = PeerDatabaseSql::peers_from_join_query(results)?;
        Ok(peers.into_iter().take(number).collect())
    }

    /// Get a peer by its node ID
    pub fn get_peer_by_node_id(&self, node_id: &NodeId) -> Result<Option<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        self.get_peer_by_node_id_inner(node_id, &mut conn)
    }

    // Get a peer by its node ID
    fn get_peer_by_node_id_inner(
        &self,
        node_id: &NodeId,
        conn: &mut SqliteConnection,
    ) -> Result<Option<Peer>, StorageError> {
        // Perform a join query to fetch peers and their addresses
        let node_id = node_id.to_hex();
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::node_id.eq(node_id))
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(conn)?;

        Ok(PeerDatabaseSql::peers_from_join_query(results)?.first().cloned())
    }

    /// Get all peers based on a list of their node_ids
    pub fn get_peers_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let node_ids_hex = node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
        self.get_peers_by_node_ids_str(&node_ids_hex, &mut conn)
    }

    /// Get all peers based on a list of their node_ids
    pub fn get_peer_public_keys_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<CommsPublicKey>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        let node_ids = node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
        let public_keys = peers::table
            .filter(peers::node_id.eq_any(node_ids))
            .select(peers::public_key)
            .load::<String>(&mut conn)?;
        let public_keys = public_keys
            .iter()
            .map(|p| CommsPublicKey::from_hex(p))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(public_keys)
    }

    fn get_peers_by_node_ids_str(
        &self,
        node_ids: &[String],
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<Peer>, StorageError> {
        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::node_id.eq_any(node_ids))
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(conn)?;

        PeerDatabaseSql::peers_from_join_query(results)
    }

    /// Get all banned peers
    pub fn get_banned_peers(&self) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::banned_until.is_not_null())
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;

        PeerDatabaseSql::peers_from_join_query(results)
    }

    /// Get a peer by its public key
    pub fn get_peer_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let public_key = public_key.to_hex();
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::public_key.eq(public_key))
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;
        if results.is_empty() {
            return Ok(None);
        }

        // Group addresses for the peer
        let peer_query = results[0].0.clone();
        let addresses_query = results.iter().map(|(_, address)| address.clone()).collect::<Vec<_>>();

        Ok(Some(Peer::try_from((peer_query, addresses_query))?))
    }

    /// Get all addresses for a peer based on its node_id
    pub fn get_addresses(&self, node_id: &NodeId) -> Result<MultiaddressesWithStats, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_id = node_id.to_hex();
            let peer_id = peers::table
                .filter(peers::node_id.eq(node_id))
                .select(peers::peer_id)
                .first::<i64>(conn)?;
            let addresses_query: Vec<NewMultiaddrWithStatsSql> = multi_addresses::table
                .filter(multi_addresses::peer_id.eq(peer_id))
                .load::<NewMultiaddrWithStatsSql>(conn)?;

            MultiaddressesWithStats::try_from(addresses_query)
        })
    }

    // Get the closest `n` not failed, banned or deleted node ids, ordered by their distance to the given node ID.
    fn get_closest_n_good_standing_peer_node_ids_inner(
        &self,
        n: usize,
        features: PeerFeatures,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<NodeId>, StorageError> {
        // Step 1: Retrieve relevant (node_ids)
        let mut query = peers::table
            .inner_join(
                multi_addresses::table.on(multi_addresses::peer_id
                    .eq(peers::peer_id)
                    .and(multi_addresses::last_failed_reason.is_null())),
            )
            .filter(peers::banned_until.is_null())
            .filter(peers::deleted_at.is_null())
            .distinct()
            .into_boxed();

        if features == PeerFeatures::COMMUNICATION_CLIENT {
            query = query.filter(peers::features.eq(features.to_i32()));
        } else {
            query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                "features & {} != 0",
                features.to_i32()
            )));
        }

        query = query
            .order_by(peers::distance_to_self.asc())
            .limit(i64::try_from(n).unwrap_or(i64::MAX));

        // Note: To debug the SQL query, uncomment the following lines:
        // --------------------------------
        // use diesel::{debug_query, sqlite::Sqlite};
        // println!();
        // println!("SQL Query: {}", debug_query::<Sqlite, _>(&query));
        // --------------------------------

        let nodes_ids_hex = query.select(peers::node_id).load::<String>(conn)?;

        let nodes_ids = nodes_ids_hex
            .into_iter()
            .filter_map(|v| NodeId::from_hex(&v).ok())
            .collect::<Vec<_>>();

        Ok(nodes_ids)
    }

    /// Get the closest `n` not failed, banned or deleted node ids, ordered by their distance to the given node ID.
    pub fn get_closest_n_good_standing_peer_node_ids(
        &self,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<NodeId>, StorageError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            self.get_closest_n_good_standing_peer_node_ids_inner(n, features, conn)
        })
    }

    /// Get the closest `n` not failed, banned or deleted peers, ordered by their distance to the given node ID.
    pub fn get_closest_n_good_standing_peers(
        &self,
        n: usize,
        features: PeerFeatures,
    ) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_ids = self.get_closest_n_good_standing_peer_node_ids_inner(n, features, conn)?;

            let node_ids_hex = node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
            let results = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::node_id.eq_any(node_ids_hex))
                .order_by(peers::distance_to_self.asc())
                .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(conn)?;

            let peers = PeerDatabaseSql::peers_from_join_query(results)?;

            Ok(peers)
        })
    }

    // Get the closest `n` active peer ids (have been seen, optionally within a threshold, not banned, not deleted,
    // optional features).
    fn get_active_peer_node_ids(
        &self,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        n: Option<usize>,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<String>, StorageError> {
        let excluded_node_ids_hex = excluded_peers.iter().map(|id| id.to_hex()).collect::<Vec<_>>();

        // Step 1: Retrieve relevant node_ids
        let mut query = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::banned_until.is_null())
            .filter(peers::deleted_at.is_null())
            .filter(peers::node_id.ne_all(excluded_node_ids_hex))
            .distinct()
            .into_boxed(); // Enables dynamic query building

        if exclude_if_all_address_failed {
            query = query
                .filter(multi_addresses::last_seen.is_not_null())
                .filter(multi_addresses::last_failed_reason.is_null());
        }

        if let Some(threshold) = stale_peer_threshold {
            let threshold = min(threshold, Duration::from_secs(i64::MAX.unsigned_abs() - 1));
            let stale_threshold =
                chrono::Utc::now().naive_utc() - chrono::Duration::from_std(threshold).unwrap_or(TimeDelta::MAX);
            query = query.filter(
                multi_addresses::last_seen
                    // Never tried to connect
                    .is_null()
                    // Or last seen after the stale threshold
                    .or(multi_addresses::last_seen.ge(stale_threshold)),
            );
        }

        if let Some(features) = features {
            if features == PeerFeatures::COMMUNICATION_CLIENT {
                query = query.filter(peers::features.eq(features.to_i32()));
            } else {
                query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                    "features & {} != 0",
                    features.to_i32()
                )));
            }
        }

        if let Some(n) = n {
            query = query
                .order_by(diesel::dsl::sql::<diesel::sql_types::Integer>("RANDOM()"))
                .limit(i64::try_from(n).unwrap_or(i64::MAX));
        }

        // Note: To debug the SQL query, uncomment the following lines:
        // --------------------------------
        // use diesel::{debug_query, sqlite::Sqlite};
        // println!();
        // println!("SQL Query: {}", debug_query::<Sqlite, _>(&query));
        // --------------------------------

        let node_ids_hex = query.select(peers::node_id).load::<String>(conn)?;

        Ok(node_ids_hex)
    }

    /// Get the closest `n` active peers (have been seen, optionally within a threshold, not banned, not deleted,
    /// optional features), ordered by their distance to the given region node ID.
    pub fn get_closest_n_active_peers(
        &self,
        region_node_id: &NodeId,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        exclusion_distance: Option<NodeDistance>,
    ) -> Result<Vec<Peer>, StorageError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_ids_hex = self.get_active_peer_node_ids(
                excluded_peers,
                features,
                stale_peer_threshold,
                exclude_if_all_address_failed,
                None,
                conn,
            )?;

            let mut node_ids = node_ids_hex
                .into_iter()
                .filter_map(|id| NodeId::from_hex(&id).ok())
                .filter(|id| {
                    exclusion_distance
                        .clone()
                        .map(|d| id.distance(region_node_id) < d)
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            node_ids.sort_by_key(|a| a.distance(region_node_id));
            node_ids.truncate(n);

            let selected_node_ids_hex = node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
            let mut peers = self.get_peers_by_node_ids_str(&selected_node_ids_hex, conn)?;

            peers.sort_by(|a, b| {
                a.node_id
                    .distance(region_node_id)
                    .cmp(&b.node_id.distance(region_node_id))
            });

            Ok(peers)
        })
    }

    /// Get `n` active random peers, ordered by their distance to the given node ID.
    pub fn get_n_random_active_peers(
        &self,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        stale_peer_threshold: Option<Duration>,
    ) -> Result<Vec<Peer>, StorageError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_ids_hex =
                self.get_active_peer_node_ids(excluded_peers, features, stale_peer_threshold, true, Some(n), conn)?;

            self.get_peers_by_node_ids_str(&node_ids_hex, conn)
        })
    }

    /// Delete all stale peers, removing them from the database and returning their node_ids
    /// - Stale Nodes:
    ///   - The node must not be identified as a node (not a client).
    ///   - A node is considered stale if:
    ///     - it has been deleted;
    ///     - all its addresses have either failed or not been seen for more than the threshold number of days.
    ///   - Seed nodes are not stale.
    ///   - Banned not deleted nodes are not stale.
    /// - Stale Wallets:
    ///   - The node must be identified as a client (not a node).
    ///   - A wallet is considered stale if:
    ///     - it has been deleted;
    ///     - none of its addresses has ever been seen;
    ///     - all its addresses have either failed or not been seen for more than the threshold number of days.
    ///   - Wallets that are considered neighbours are not stale, except if they were deleted.
    #[allow(clippy::too_many_lines)]
    pub fn delete_all_stale_peers(
        &self,
        stale_peer_threshold: Duration,
        neighbours_count: usize,
    ) -> Result<Vec<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let stale_threshold = chrono::Utc::now().naive_utc() -
                chrono::Duration::from_std(stale_peer_threshold).unwrap_or(TimeDelta::MAX);

            // Identify stale nodes
            use diesel::{prelude::*, sql_types, sql_types::Text};
            #[derive(Debug, QueryableByName)]
            struct NodeIdRow {
                #[diesel(sql_type = Text)]
                node_id: String,
            }

            let stale_nodes_hex = diesel::sql_query(r#"
                -- Deleted node peers: always considered stale (excluding SEEDs)
                SELECT peers.node_id
                FROM peers
                WHERE peers.features = ?
                    AND peers.flags != ?
                    AND peers.deleted_at IS NOT NULL

                UNION

                -- Active peers with only stale or failed addresses (excluding SEEDs)
                SELECT peers.node_id
                FROM peers
                INNER JOIN multi_addresses ON multi_addresses.peer_id = peers.peer_id
                WHERE peers.features = ?
                  AND peers.flags != ?
                  AND peers.deleted_at IS NULL
                GROUP BY peers.node_id
                HAVING
                    -- All associated addresses are either failed or stale (not recently seen)
                    SUM(
                        CASE
                            WHEN multi_addresses.last_failed_reason IS NULL
                              AND (
                                multi_addresses.last_seen IS NULL
                                OR multi_addresses.last_seen >= ?
                              )
                            THEN 1 ELSE 0
                        END
                    ) = 0

            "#)
                .bind::<sql_types::Integer, _>(PeerFeatures::COMMUNICATION_NODE.to_i32())  // for WHERE
                .bind::<sql_types::Integer, _>(PeerFlags::SEED.to_i32())                   // for WHERE
                .bind::<sql_types::Integer, _>(PeerFeatures::COMMUNICATION_NODE.to_i32())  // for second WHERE
                .bind::<sql_types::Integer, _>(PeerFlags::SEED.to_i32())                   // for second WHERE
                .bind::<sql_types::Timestamp, _>(stale_threshold)                          // for HAVING
                .load::<NodeIdRow>(conn)?;

            let stale_nodes_hex: Vec<String> = stale_nodes_hex.into_iter().map(|row| row.node_id).collect();

            // Step 2: Identify stale wallets
            let stale_wallets_hex = diesel::sql_query(r#"
                -- Deleted wallet peers: always considered stale
                SELECT peers.node_id
                FROM peers
                WHERE peers.features = ?
                    AND peers.deleted_at IS NOT NULL

                UNION

                -- Active peers with only stale or failed addresses
                SELECT peers.node_id
                FROM peers
                INNER JOIN multi_addresses ON multi_addresses.peer_id = peers.peer_id
                WHERE peers.features = ?
                  AND peers.deleted_at IS NULL
                GROUP BY peers.node_id
                HAVING
                    -- All associated addresses are either failed or stale (not recently seen)
                    SUM(
                        CASE
                            WHEN multi_addresses.last_failed_reason IS NULL
                              AND multi_addresses.last_seen >= ?
                            THEN 1 ELSE 0
                        END
                    ) = 0
            "#)
                .bind::<sql_types::Integer, _>(PeerFeatures::COMMUNICATION_CLIENT.to_i32())  // for WHERE
                .bind::<sql_types::Integer, _>(PeerFeatures::COMMUNICATION_CLIENT.to_i32())  // for second WHERE
                .bind::<sql_types::Timestamp, _>(stale_threshold)                          // for HAVING
                .load::<NodeIdRow>(conn)?;

            let mut stale_wallets_hex: Vec<String> = stale_wallets_hex.into_iter().map(|row| row.node_id).collect();
            let mut stale_wallets = stale_wallets_hex
                .iter()
                .flat_map(|id| NodeId::from_hex(id).ok())
                .collect::<Vec<_>>();
            stale_wallets.sort_by_key(|a| a.distance(&self.this_peer_identity.node_id));

            // Step 3: Exclude closest wallet peers that are not deleted
            let mut neighbour_wallets = stale_wallets.clone();
            let deleted_peers = self.get_all_deleted_peers(Some(PeerFeatures::COMMUNICATION_CLIENT), conn)?;
            neighbour_wallets.retain(|id| !deleted_peers.contains(id));
            neighbour_wallets.truncate(neighbours_count);
            let neighbour_wallets_hex = neighbour_wallets.into_iter().map(|id| id.to_hex()).collect::<Vec<_>>();
            stale_wallets_hex.retain(|id| !neighbour_wallets_hex.contains(id));

            // Step 4: Delete stale nodes and wallets
            let stale_peers = stale_nodes_hex.into_iter().chain(stale_wallets_hex).collect::<Vec<_>>();
            diesel::delete(
                multi_addresses::table.filter(
                    multi_addresses::peer_id.eq_any(
                        peers::table
                            .filter(peers::node_id.eq_any(&stale_peers))
                            .select(peers::peer_id),
                    ),
                ),
            )
            .execute(conn)?;
            diesel::delete(peers::table.filter(peers::node_id.eq_any(stale_peers.clone()))).execute(conn)?;

            // Step 5: Retain at most the threshold number of wallets
            let mut remaining_wallets = self
                .get_all_peers_inner(Some(PeerFeatures::COMMUNICATION_CLIENT), conn)?
                .iter()
                .map(|p| p.node_id.clone())
                .collect::<Vec<_>>();
            remaining_wallets.sort_by_key(|a| a.distance(&self.this_peer_identity.node_id));
            let surplus_wallets = remaining_wallets.iter().skip(neighbours_count).collect::<Vec<_>>();
            let surplus_wallets_hex = surplus_wallets.iter().map(|id| id.to_hex()).collect::<Vec<_>>();

            diesel::delete(
                multi_addresses::table.filter(
                    multi_addresses::peer_id.eq_any(
                        peers::table
                            .filter(peers::node_id.eq_any(&surplus_wallets_hex))
                            .select(peers::peer_id),
                    ),
                ),
            )
            .execute(conn)?;
            diesel::delete(peers::table.filter(peers::node_id.eq_any(surplus_wallets_hex.clone()))).execute(conn)?;
            let stale_peers = stale_peers.into_iter().chain(surplus_wallets_hex).collect::<Vec<_>>();

            // Step 5: Return all deleted node_ids
            Ok(stale_peers
                .into_iter()
                .filter_map(|node_id| NodeId::from_hex(&node_id).ok())
                .collect::<Vec<_>>())
        })
    }

    /// Get a random set of `n` peers from the database that are not banned and not deleted
    pub fn get_n_random_peers(&self, n: usize, exclude_node_ids: &[NodeId]) -> Result<Vec<Peer>, StorageError> {
        if n == 0 {
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;
        let exclude_node_ids = exclude_node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();

        conn.transaction::<_, StorageError, _>(|conn| {
            // Step 1: Filtered, random and truncated list of node_ids
            let node_ids: Vec<String> = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::deleted_at.is_null())
                .filter(peers::banned_until.is_null().or(peers::banned_until.lt(now.nullable())))
                .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                    "features & {} != 0",
                    PeerFeatures::COMMUNICATION_NODE.to_i32()
                )))
                .filter(multi_addresses::last_seen.is_not_null())
                .filter(peers::node_id.ne_all(exclude_node_ids))
                .order_by(diesel::dsl::sql::<diesel::sql_types::Integer>("RANDOM()"))
                .limit(i64::try_from(n).unwrap_or(i64::MAX))
                .select(peers::node_id)
                .distinct()
                .load::<String>(conn)?;

            if node_ids.is_empty() {
                return Ok(Vec::new());
            }

            // Step 2: Load full peer + addresses only for selected node_ids
            self.get_peers_by_node_ids_str(&node_ids, conn)
        })
    }

    /// Get all the seed peers
    pub fn get_seed_peers(&self) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::flags.eq(PeerFlags::SEED.to_i32()))
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;

        PeerDatabaseSql::peers_from_join_query(results)
    }

    // Retrieve the peer indexes as 'Vec<(peer_id, public_key, node_id)>'
    fn get_peer_indexes(&self) -> Result<Vec<(i64, String, String)>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        let peer_indexes = peers::table
            .select((peers::peer_id, peers::public_key, peers::node_id))
            .load::<(i64, String, String)>(&mut conn)?;
        Ok(peer_indexes.into_iter().collect::<Vec<_>>())
    }

    /// Get the size of the peer database
    pub fn size(&self) -> usize {
        self.get_peer_indexes().unwrap_or_default().len()
    }
}

fn sql_escape(input: &str) -> String {
    input.replace('\'', "''")
}

#[derive(Clone, Debug, Selectable, Queryable, Insertable, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = node_identity)]
pub struct NewThisPeerIdentitySql {
    pub public_key: String,
    pub node_id: String,
    pub features: i32,
}

#[derive(Clone, Debug)]
pub struct NewPeerWithAddressesSql {
    pub peer: NewPeerSql,
    pub addresses: Vec<NewMultiaddrWithStatsSql>,
}

#[derive(Clone, Debug, Selectable, Queryable, Insertable, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = peers)]
pub struct NewPeerSql {
    pub peer_id: i64,
    pub public_key: String,
    pub node_id: String,
    pub distance_to_self: String,
    pub flags: i32,
    pub banned_until: Option<chrono::NaiveDateTime>,
    pub banned_reason: Option<String>,
    pub features: i32,
    pub supported_protocols: String,
    pub added_at: chrono::NaiveDateTime,
    pub user_agent: String,
    pub metadata: Option<Vec<u8>>,
    pub deleted_at: Option<chrono::NaiveDateTime>,
}

#[derive(Clone, Debug, Selectable, Queryable, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = peers)]
pub struct UpdatePeerSql {
    pub node_id: String,
    pub banned_until: Option<chrono::NaiveDateTime>,
    pub banned_reason: Option<String>,
    pub supported_protocols: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Option<Vec<u8>>,
    pub deleted_at: Option<chrono::NaiveDateTime>,
}

#[derive(Clone, Debug)]
pub struct UpdatePeerWithAddressesSql {
    pub peer: UpdatePeerSql,
    pub addresses: Vec<UpdateMultiaddrWithStatsSql>,
}

#[derive(Clone, Debug, Selectable, Queryable, Insertable, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = multi_addresses)]
pub struct NewMultiaddrWithStatsSql {
    pub address_id: Option<i32>,
    pub peer_id: i64,
    pub address: String,
    pub last_seen: Option<chrono::NaiveDateTime>,
    pub connection_attempts: Option<i32>,
    pub avg_initial_dial_time: Option<i64>,
    pub initial_dial_time_sample_count: Option<i32>,
    pub avg_latency: Option<i64>,
    pub latency_sample_count: Option<i32>,
    pub last_attempted: Option<chrono::NaiveDateTime>,
    pub last_failed_reason: Option<String>,
    pub quality_score: Option<i32>,
    pub source: String,
}

#[derive(Clone, Debug, Selectable, Queryable, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = multi_addresses)]
pub struct UpdateMultiaddrWithStatsSql {
    pub address: String,
    pub last_seen: Option<chrono::NaiveDateTime>,
    pub connection_attempts: Option<i32>,
    pub avg_initial_dial_time: Option<i64>,
    pub initial_dial_time_sample_count: Option<i32>,
    pub avg_latency: Option<i64>,
    pub latency_sample_count: Option<i32>,
    pub last_attempted: Option<chrono::NaiveDateTime>,
    pub last_failed_reason: Option<String>,
    pub quality_score: Option<i32>,
    pub source: Option<String>,
}

/// Serialize the protocols into a comma-separated string
pub fn serialize_protocols(protocols: &[ProtocolId]) -> String {
    protocols
        .iter()
        .map(|p| String::from_utf8_lossy(p).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Deserialize the protocols from a comma-separated string
pub fn deserialize_protocols(data: &str) -> Vec<ProtocolId> {
    if data.is_empty() {
        Vec::new()
    } else {
        data.split(',').map(|s| Bytes::from(s.to_string())).collect()
    }
}
/// Serialize the metadata into a `Vec<u8>`, mapping empty to `None`
pub fn serialize_metadata(metadata: &HashMap<u8, Vec<u8>>) -> Result<Option<Vec<u8>>, StorageError> {
    if metadata.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::to_vec(metadata)?))
    }
}

/// Deserialize the metadata from a `Vec<u8>`, mapping empty to `None`
pub fn deserialize_metadata(data: Option<Vec<u8>>) -> Result<HashMap<u8, Vec<u8>>, StorageError> {
    match data {
        Some(d) if !d.is_empty() => serde_json::from_slice(&d).map_err(StorageError::JsonError),
        _ => Ok(HashMap::new()),
    }
}

impl From<MultiaddrWithStats> for UpdateMultiaddrWithStatsSql {
    fn from(address: MultiaddrWithStats) -> Self {
        UpdateMultiaddrWithStatsSql::from(&address)
    }
}

fn duration_to_i64_ms_infallible(duration: Option<Duration>) -> Option<i64> {
    match duration.map(|v| v.as_millis()) {
        Some(ms_u128) => match ms_u128.try_into() {
            Ok(ms_i64) => Some(ms_i64),
            Err(e) => {
                warn!(target: LOG_TARGET, "duration_to_i64_ms_infallible {:?} conversion error: {}", duration, e);
                Some(i64::MAX)
            },
        },
        _ => None,
    }
}

fn u32_to_i32_infallible(value: u32) -> i32 {
    i32::try_from(value).unwrap_or({
        warn!(target: LOG_TARGET, "u32_to_i32_infallible conversion error");
        i32::MAX
    })
}

impl From<&MultiaddrWithStats> for UpdateMultiaddrWithStatsSql {
    fn from(address: &MultiaddrWithStats) -> Self {
        UpdateMultiaddrWithStatsSql {
            address: address.to_string(),
            last_seen: address.last_seen(),
            connection_attempts: Some(u32_to_i32_infallible(address.connection_attempts())),
            avg_initial_dial_time: duration_to_i64_ms_infallible(address.avg_initial_dial_time()),
            initial_dial_time_sample_count: Some(u32_to_i32_infallible(address.initial_dial_time_sample_count())),
            avg_latency: duration_to_i64_ms_infallible(address.avg_latency()),
            latency_sample_count: Some(u32_to_i32_infallible(address.latency_sample_count())),
            last_attempted: address.last_attempted(),
            last_failed_reason: address.last_failed_reason().map(|v| v.to_string()),
            quality_score: address.quality_score(),
            source: Some(serde_json::to_string(&address.source()).unwrap_or_default()),
        }
    }
}

impl TryFrom<(NewPeerSql, Vec<NewMultiaddrWithStatsSql>)> for Peer {
    type Error = StorageError;

    fn try_from(
        (peer_query, addresses_query): (NewPeerSql, Vec<NewMultiaddrWithStatsSql>),
    ) -> Result<Self, Self::Error> {
        Ok(Peer::new_with_stats(
            Some(
                u64::try_from(peer_query.peer_id)
                    .expect("infallible - auto generated from 'generate_peer_id_as_i64()'"),
            )
            .filter(|&id| id != 0),
            CommsPublicKey::from_hex(&peer_query.public_key)?,
            NodeId::from_hex(&peer_query.node_id)?,
            MultiaddressesWithStats::try_from(addresses_query)?,
            PeerFlags::from_bits(u8::try_from(peer_query.flags)?)
                .ok_or_else(|| StorageError::UnexpectedResult("Peer flags are invalid".to_string()))?,
            peer_query.banned_until,
            peer_query.banned_reason.unwrap_or_default(),
            PeerFeatures::from_bits(u32::try_from(peer_query.features)?)
                .ok_or_else(|| StorageError::UnexpectedResult("Peer features are invalid".to_string()))?,
            deserialize_protocols(&peer_query.supported_protocols),
            peer_query.added_at,
            peer_query.user_agent,
            deserialize_metadata(peer_query.metadata)?,
            peer_query.deleted_at,
        ))
    }
}

fn i64_to_duration(val: Option<i64>) -> Result<Option<Duration>, StorageError> {
    val.map(|t| {
        u64::try_from(t)
            .map(Duration::from_millis)
            .map_err(|_| StorageError::UnexpectedResult("Invalid duration".to_string()))
    })
    .transpose()
}

impl TryFrom<Vec<NewMultiaddrWithStatsSql>> for MultiaddressesWithStats {
    type Error = StorageError;

    fn try_from(addresses_query: Vec<NewMultiaddrWithStatsSql>) -> Result<Self, Self::Error> {
        let mut addresses = Vec::new();
        for addr in addresses_query {
            let address = MultiaddrWithStats::new_with_stats(
                Multiaddr::from_str(&addr.address).map_err(|e| StorageError::UnexpectedResult(e.to_string()))?,
                addr.last_seen,
                u32::try_from(addr.connection_attempts.unwrap_or_default())?,
                i64_to_duration(addr.avg_initial_dial_time)?,
                u32::try_from(addr.initial_dial_time_sample_count.unwrap_or_default())?,
                i64_to_duration(addr.avg_latency)?,
                u32::try_from(addr.latency_sample_count.unwrap_or_default())?,
                addr.last_attempted,
                addr.last_failed_reason,
                addr.quality_score,
                serde_json::from_str(&addr.source).map_err(StorageError::JsonError)?,
            );
            addresses.push(address);
        }
        Ok(MultiaddressesWithStats::from(addresses))
    }
}

impl From<(UpdateMultiaddrWithStatsSql, i64)> for NewMultiaddrWithStatsSql {
    fn from((address, peer_id): (UpdateMultiaddrWithStatsSql, i64)) -> Self {
        NewMultiaddrWithStatsSql {
            address_id: None,
            peer_id,
            address: address.address,
            last_seen: address.last_seen,
            connection_attempts: address.connection_attempts,
            avg_initial_dial_time: address.avg_initial_dial_time,
            initial_dial_time_sample_count: address.initial_dial_time_sample_count,
            avg_latency: address.avg_latency,
            latency_sample_count: address.latency_sample_count,
            last_attempted: address.last_attempted,
            last_failed_reason: address.last_failed_reason,
            quality_score: address.quality_score,
            source: address.source.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use chrono::TimeDelta;
    use diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl};
    use digest::crypto_common::rand_core::OsRng;
    use rand::seq::SliceRandom;
    use tari_common_sqlite::connection::DbConnection;
    use tari_utilities::{hex::Hex, ByteArray};

    use crate::{
        net_address::{MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{
            create_test_peer,
            database::{NewMultiaddrWithStatsSql, NewPeerSql, PeerDatabaseSql, MIGRATIONS},
            storage::{
                database::{duration_to_i64_ms_infallible, u32_to_i32_infallible},
                schema::{multi_addresses, peers},
            },
            NodeId,
            Peer,
            PeerFeatures,
            PeerFlags,
        },
        protocol::ProtocolId,
        types::CommsPublicKey,
    };

    #[test]
    fn test_add_update_peer_with_addresses() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create a new peer
        let mut new_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);

        // Add the peer to the database
        let mut new_peer_sql = peers_db.add_peer_sql(new_peer.clone()).unwrap();
        peers_db.add_or_update_peer(new_peer.clone()).unwrap();

        // Verify the peer was added
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let count: i64 = peers::table.count().get_result(&mut conn).unwrap();
        assert_eq!(count, 1);
        let count: i64 = multi_addresses::table.count().get_result(&mut conn).unwrap();
        assert_eq!(count, i64::try_from(new_peer.addresses.len()).unwrap());

        // Verify the peer sql data
        let peer_query: NewPeerSql = peers::table
            .filter(peers::node_id.eq(new_peer.node_id.to_hex()))
            .first::<NewPeerSql>(&mut conn)
            .unwrap();
        let peer_id: i64 = peers::table
            .filter(peers::node_id.eq(new_peer_sql.peer.node_id.clone()))
            .select(peers::peer_id)
            .first::<i64>(&mut conn)
            .unwrap();
        new_peer_sql.peer.peer_id = peer_id;
        assert_eq!(peer_query, new_peer_sql.peer);

        // Verify the peer's multi-addresses sql data
        let addresses_query: Vec<NewMultiaddrWithStatsSql> = multi_addresses::table
            .filter(multi_addresses::peer_id.eq(peer_query.peer_id))
            .load::<NewMultiaddrWithStatsSql>(&mut conn)
            .unwrap();
        for (address_query, mut address) in addresses_query.iter().zip(new_peer_sql.addresses) {
            address.address_id = address_query.address_id;
            address.peer_id = peer_id;
            assert_eq!(address_query, &address);
        }

        // Verify the test peer can be reconstructed from the queries
        let peer_from_query = Peer::try_from((peer_query, addresses_query)).unwrap();
        assert_eq!(peer_from_query, new_peer);

        // Verify the peer can be retrieved from the db by node_id
        let peer_from_db = peers_db.get_peer_by_node_id(&new_peer.node_id).unwrap().unwrap();
        assert_eq!(peer_from_db, new_peer);

        // Update peer data
        // - new peer stats
        new_peer.ban_for(Duration::from_secs(12345), "Misbehave".to_string());
        new_peer
            .supported_protocols
            .push(ProtocolId::from_static(b"Test Protocol 1.0"));
        new_peer.metadata.insert(1, vec![1, 2, 3]);
        new_peer.metadata.insert(2, vec![4, 5, 6]);
        // - add another multi-address
        let new_addr_str = "/ip4/127.0.0.1/udt/sctp/5678";
        new_peer
            .addresses
            .add_address(&new_addr_str.parse().unwrap(), &PeerAddressSource::Config);
        // - new stats for the first multi-address
        let mut address_to_update = new_peer.addresses.addresses().first().unwrap().clone();
        address_to_update.update_latency(Duration::from_millis(123));
        address_to_update.update_initial_dial_time(Duration::from_millis(1234));
        address_to_update.mark_last_seen_now();
        new_peer
            .addresses
            .merge(&MultiaddressesWithStats::new(vec![address_to_update.clone()]));

        // Update the peer in the database
        peers_db.add_or_update_peer(new_peer.clone()).unwrap();

        // Verify the updated peer can be retrieved from the db by node_id
        let peer_from_db = peers_db.get_peer_by_node_id(&new_peer.node_id).unwrap().unwrap();
        assert_eq!(peer_from_db, new_peer);
        assert_eq!(peer_from_db.addresses, new_peer.addresses);

        // Verify that the addresses can be retrieved from the db by node_id
        let addresses_from_db = peers_db.get_addresses(&new_peer.node_id).unwrap();
        assert_eq!(addresses_from_db, new_peer.addresses);
    }

    #[ignore]
    #[test]
    fn test_batch_add_update_peers_with_addresses() {
        // let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        // let peers_db = PeerDatabaseSql::new(
        //     db_connection,
        //     &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        // )
        // .unwrap();
        //
        // // Step 1: Create peers
        // let mut new_peers = Vec::new();
        // for _ in 0..10 {
        //     let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        //     new_peers.push(peer);
        // }
        //
        // // Step 2: Batch add peers
        // let peers_with_addresses = new_peers
        //     .iter()
        //     .map(|p| peers_db.add_peer_sql(p.clone()).unwrap())
        //     .collect::<Vec<_>>();
        // let added_count = peers_db
        //     .batch_add_peers_with_addresses(peers_with_addresses.clone())
        //     .unwrap();
        // assert_eq!(added_count, 10);
        //
        // // Step 3: Verify peers were added
        // let all_peers = peers_db.get_all_peers(None).unwrap();
        // assert_eq!(all_peers.len(), 10);
        //
        // // Step 4: Update all peers
        // let mut updated_peers_with_addresses = Vec::new();
        // for peer in &mut new_peers {
        //     // - new peer stats
        //     peer.ban_for(
        //         Duration::from_secs(rand::thread_rng().gen_range(1000..9000)),
        //         "Misbehave".to_string(),
        //     );
        //     peer.supported_protocols
        //         .push(ProtocolId::from_static(b"Test Protocol 1.0"));
        //     peer.metadata
        //         .insert(1, vec![1, 2, rand::thread_rng().gen_range(1..100)]);
        //     peer.metadata
        //         .insert(2, vec![4, 5, rand::thread_rng().gen_range(1..100)]);
        //     // - add another multi-address
        //     let n = [
        //         rand::thread_rng().gen_range(1..9),
        //         rand::thread_rng().gen_range(1..9),
        //         rand::thread_rng().gen_range(1..9),
        //         rand::thread_rng().gen_range(1..9),
        //     ];
        //     let new_addr_str = format!("/ip4/{}.{}.{}.{}/udt/sctp/{0}{1}{2}{3}", n[0], n[1], n[2], n[3]);
        //     peer.addresses
        //         .add_address(&new_addr_str.parse().unwrap(), &PeerAddressSource::Config);
        //     // - new stats for the first multi-address
        //     let mut address_to_update = peer.addresses.addresses().first().unwrap().clone();
        //     address_to_update.update_latency(Duration::from_millis(rand::thread_rng().gen_range(100..1000)));
        //     address_to_update.update_initial_dial_time(Duration::from_millis(rand::thread_rng().gen_range(100..
        // 1000)));     address_to_update.mark_last_seen_now();
        //     peer.addresses
        //         .merge(&MultiaddressesWithStats::new(vec![address_to_update.clone()]));
        //
        //     let update_peer_sql = PeerDatabaseSql::update_peer_sql(peer.clone()).unwrap();
        //     updated_peers_with_addresses.push(update_peer_sql);
        // }
        //
        // // Step 5: Batch update peers
        // peers_db
        //     .batch_update_peers_with_addresses(updated_peers_with_addresses)
        //     .unwrap();
        //
        // // Step 6: Verify updates
        // let all_peers = peers_db.get_all_peers(None).unwrap();
        // for peer in all_peers {
        //     assert!(peer.is_banned());
        //     assert!(peer.metadata.contains_key(&2));
        //     assert!(peer
        //         .addresses
        //         .addresses()
        //         .iter()
        //         .any(|addr| addr.address().to_string().contains("/udt/sctp/")));
        // }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_peer_features() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node peers
        let mut node_peers = Vec::with_capacity(12);
        for i in 0..12 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            if i % 4 == 0 {
                peer.flags = PeerFlags::SEED;
            }
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }
        // Create new wallet peers
        let mut wallet_peers = Vec::with_capacity(12);
        for _i in 0..12 {
            let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT);
            wallet_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }

        let closest_nodes = peers_db
            .get_closest_n_active_peers(
                &node_peers[5].node_id,
                5,
                &[node_peers[6].node_id.clone(), node_peers[7].node_id.clone()],
                Some(PeerFeatures::MESSAGE_PROPAGATION),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_nodes.len(), 5);

        let closest_nodes = peers_db
            .get_closest_n_active_peers(
                &node_peers[5].node_id,
                5,
                &[node_peers[6].node_id.clone(), node_peers[7].node_id.clone()],
                Some(PeerFeatures::DHT_STORE_FORWARD),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_nodes.len(), 5);

        let closest_nodes = peers_db
            .get_closest_n_active_peers(
                &node_peers[5].node_id,
                5,
                &[node_peers[6].node_id.clone(), node_peers[7].node_id.clone()],
                Some(PeerFeatures::COMMUNICATION_NODE),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_nodes.len(), 5);

        // Test 'get_closest_n_active_peers' - wallets
        let closest_peers = peers_db
            .get_closest_n_active_peers(
                &wallet_peers[5].node_id,
                5,
                &[wallet_peers[6].node_id.clone(), wallet_peers[7].node_id.clone()],
                Some(PeerFeatures::COMMUNICATION_CLIENT),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_peers.len(), 5);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_various_queries() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node peers
        let mut node_peers = Vec::with_capacity(12);
        for i in 0..12 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            if i % 4 == 0 {
                peer.flags = PeerFlags::SEED;
            }
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }
        // Create new wallet peers
        let mut wallet_peers = Vec::with_capacity(12);
        for _i in 0..12 {
            let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT);
            wallet_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }

        // Test 'get_peer_indexes'
        assert_eq!(peers_db.size(), 24);

        // Test 'find_all_peers_match_partial_key'
        for i in 1..NodeId::byte_size() {
            let matches = peers_db
                .find_all_peers_match_partial_key(&node_peers[0].node_id.as_bytes()[0..i])
                .unwrap();
            assert!(matches.contains(&node_peers[0]));
        }
        for i in 1..CommsPublicKey::key_length() {
            let matches = peers_db
                .find_all_peers_match_partial_key(&node_peers[0].public_key.as_bytes()[0..i])
                .unwrap();
            assert!(matches.contains(&node_peers[0]));
        }

        // Test 'set_deleted_at'
        peers_db.set_deleted_at(&node_peers[1].node_id).unwrap();
        let deleted_peer = peers_db.get_peer_by_node_id(&node_peers[1].node_id).unwrap().unwrap();
        assert!(deleted_peer.deleted_at.is_some());

        peers_db.set_deleted_at(&wallet_peers[1].node_id).unwrap();
        let deleted_peer = peers_db.get_peer_by_node_id(&wallet_peers[1].node_id).unwrap().unwrap();
        assert!(deleted_peer.deleted_at.is_some());

        // Test 'peer_exists_by_node_id'
        assert!(peers_db
            .peer_exists_by_node_id(&node_peers[2].node_id)
            .unwrap()
            .is_some());

        // Test 'get_peer_by_public_key'
        let peer = peers_db
            .get_peer_by_public_key(&node_peers[3].public_key)
            .unwrap()
            .unwrap();
        assert_eq!(peer, node_peers[3]);

        // Test 'peer_exists_by_public_key'
        assert!(peers_db
            .peer_exists_by_public_key(&node_peers[4].public_key)
            .unwrap()
            .is_some());

        // Test 'get_all_peers'
        let all_peers = peers_db.get_all_peers(None).unwrap();
        assert_eq!(all_peers.len(), 24);

        // Test 'get_n_not_banned_or_deleted_peers'
        peers_db
            .set_banned(
                &node_peers[4].node_id,
                Duration::from_secs(12345),
                "Misbehaviour is punished".to_string(),
            )
            .unwrap();
        peers_db
            .set_banned(
                &wallet_peers[4].node_id,
                Duration::from_secs(12345),
                "Misbehaviour is punished".to_string(),
            )
            .unwrap();
        let n_peers = peers_db.get_n_not_banned_or_deleted_peers(24).unwrap();
        // node peer 1 is deleted, node peer 4 is banned
        // wallet peer 1 is deleted, wallet peer 4 is banned
        assert_eq!(n_peers.len(), 20);
        assert!(!n_peers
            .iter()
            .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id));
        assert!(!n_peers
            .iter()
            .any(|n| n.node_id == wallet_peers[1].node_id || n.node_id == wallet_peers[4].node_id));

        // Test 'set_last_seen'
        let last_seen = chrono::Utc::now().naive_utc() -
            chrono::Duration::from_std(Duration::from_secs(120)).unwrap_or(TimeDelta::MAX);
        for address in node_peers[8].addresses.addresses() {
            peers_db
                .set_last_seen(&node_peers[8].node_id, last_seen, address.address())
                .unwrap();
        }
        assert_eq!(
            peers_db
                .get_peer_by_node_id(&node_peers[8].node_id)
                .unwrap()
                .unwrap()
                .last_seen()
                .unwrap(),
            last_seen
        );
        for address in wallet_peers[8].addresses.addresses() {
            peers_db
                .set_last_seen(&wallet_peers[8].node_id, last_seen, address.address())
                .unwrap();
        }
        assert_eq!(
            peers_db
                .get_peer_by_node_id(&wallet_peers[8].node_id)
                .unwrap()
                .unwrap()
                .last_seen()
                .unwrap(),
            last_seen
        );

        // Test 'get_closest_n_active_peers' - nodes
        let closest_nodes = peers_db
            .get_closest_n_active_peers(
                &node_peers[5].node_id,
                5,
                &[node_peers[6].node_id.clone(), node_peers[7].node_id.clone()],
                Some(PeerFeatures::COMMUNICATION_NODE),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_nodes.len(), 5);
        // Verify deleted & banned
        assert!(!closest_nodes
            .iter()
            .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id));
        // Verify stale
        assert!(!closest_nodes.iter().any(|n| n.node_id == node_peers[8].node_id));
        // Verify excluded
        assert!(!closest_nodes
            .iter()
            .any(|n| n.node_id == node_peers[6].node_id || n.node_id == node_peers[7].node_id));
        // Verify all are nodes
        assert!(closest_nodes.iter().all(|n| n.features.is_node()));
        // Verify sorting by distance
        for i in 0..closest_nodes.len() - 1 {
            assert!(
                closest_nodes[i].node_id.distance(&node_peers[5].node_id) <=
                    closest_nodes[i + 1].node_id.distance(&node_peers[5].node_id)
            );
        }

        // Test 'get_closest_n_active_peer_node_ids' - nodes
        let closest_peers = peers_db
            .get_closest_n_active_peers(
                &node_peers[5].node_id,
                5,
                &[node_peers[6].node_id.clone(), node_peers[7].node_id.clone()],
                Some(PeerFeatures::COMMUNICATION_NODE),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_peers.len(), 5);
        // Verify deleted & banned
        assert!(!closest_peers
            .iter()
            .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id));
        // Verify stale
        assert!(!closest_peers.iter().any(|n| n.node_id == node_peers[8].node_id));
        // Verify excluded
        assert!(!closest_peers
            .iter()
            .any(|n| n.node_id == node_peers[6].node_id || n.node_id == node_peers[7].node_id));
        // Verify all are nodes
        let node_ids_from_closest_nodes = closest_nodes.iter().map(|n| n.node_id.clone()).collect::<Vec<_>>();
        assert!(closest_peers
            .iter()
            .all(|n| node_ids_from_closest_nodes.contains(&n.node_id)));
        // Verify sorting by distance
        for i in 0..closest_peers.len() - 1 {
            assert!(
                closest_peers[i].node_id.distance(&node_peers[5].node_id) <=
                    closest_peers[i + 1].node_id.distance(&node_peers[5].node_id)
            );
        }

        // Test 'get_closest_n_active_peers' - wallets
        let closest_peers = peers_db
            .get_closest_n_active_peers(
                &wallet_peers[5].node_id,
                5,
                &[wallet_peers[6].node_id.clone(), wallet_peers[7].node_id.clone()],
                Some(PeerFeatures::COMMUNICATION_CLIENT),
                Some(Duration::from_secs(60)),
                true,
                None,
            )
            .unwrap();
        assert_eq!(closest_peers.len(), 5);
        // Verify deleted & banned
        assert!(!closest_peers
            .iter()
            .any(|n| n.node_id == wallet_peers[1].node_id || n.node_id == wallet_peers[4].node_id));
        // Verify stale
        assert!(!closest_peers.iter().any(|n| n.node_id == wallet_peers[8].node_id));
        // Verify excluded
        assert!(!closest_peers
            .iter()
            .any(|n| n.node_id == wallet_peers[6].node_id || n.node_id == wallet_peers[7].node_id));
        // Verify all are nodes
        assert!(closest_peers.iter().all(|n| n.features.is_client()));
        // Verify sorting by distance
        for i in 0..closest_peers.len() - 1 {
            assert!(
                closest_peers[i].node_id.distance(&wallet_peers[5].node_id) <=
                    closest_peers[i + 1].node_id.distance(&wallet_peers[5].node_id)
            );
        }

        // Test 'get_seed_peers'
        let seed_peers = peers_db.get_seed_peers().unwrap();
        assert_eq!(seed_peers.len(), 3);
        for peer in &seed_peers {
            assert!(peer.is_seed());
        }

        // Test 'random_peers_sqlite'
        let random_peers = peers_db
            .get_n_random_peers(5, &[node_peers[0].node_id.clone()])
            .unwrap();
        assert_eq!(random_peers.len(), 5);
        // Verify deleted & banned
        assert!(!random_peers
            .iter()
            .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id));
        // Verify excluded
        assert!(!random_peers.iter().any(|n| n.node_id == node_peers[0].node_id));

        // Test resets
        // - banned, last_seen
        let peer = peers_db.get_peer_by_node_id(&node_peers[4].node_id).unwrap().unwrap();
        assert!(peer.is_banned());
        assert!(peer.last_seen().is_some());
        peers_db.reset_banned(&node_peers[4].node_id).unwrap();
        for address in peer.addresses.address_iter() {
            peers_db.reset_last_seen(&node_peers[4].node_id, address).unwrap();
        }
        let peer = peers_db.get_peer_by_node_id(&node_peers[4].node_id).unwrap().unwrap();
        assert!(!peer.is_banned());
        assert!(peer.last_seen().is_none());
        for peer in node_peers.iter().chain(wallet_peers.iter()) {
            peers_db
                .set_banned(
                    &peer.node_id,
                    Duration::from_secs(12345),
                    "Misbehaviour is punished".to_string(),
                )
                .unwrap();
        }
        let all_peers = peers_db.get_all_peers(None).unwrap();
        for peer in &all_peers {
            assert!(peer.is_banned());
        }
        peers_db.reset_all_banned().unwrap();
        let all_peers = peers_db.get_all_peers(None).unwrap();
        for peer in &all_peers {
            assert!(!peer.is_banned());
        }

        // - reset_all_offline_peers
        for peer in &node_peers {
            let mut peer = peer.clone();
            let addresses = peer.addresses.addresses().to_vec();
            for address in &addresses {
                peer.addresses
                    .mark_failed_connection_attempt(address.address(), "Misbehave".to_string());
            }
            peers_db.add_or_update_peer(peer.clone()).unwrap();
        }
        let all_peers = peers_db.get_all_peers(Some(PeerFeatures::COMMUNICATION_NODE)).unwrap();
        for peer in &all_peers {
            assert!(peer.last_connect_attempt().is_some());
        }
        peers_db.reset_offline_non_wallet_peers().unwrap();
        let all_peers = peers_db.get_all_peers(Some(PeerFeatures::COMMUNICATION_NODE)).unwrap();
        for peer in &all_peers {
            assert!(peer.last_connect_attempt().is_none(), "peer: {}", peer);
        }

        // - last_failed_reason
        for address in node_peers[11].addresses.addresses() {
            peers_db
                .set_last_failed_reason(
                    &node_peers[11].node_id,
                    "not playing with".to_string(),
                    address.address(),
                )
                .unwrap();
        }
        let peer = peers_db.get_peer_by_node_id(&node_peers[11].node_id).unwrap().unwrap();
        assert!(peer.all_addresses_failed());
        for address in peer.addresses.address_iter() {
            peers_db
                .reset_last_failed_reason(&node_peers[11].node_id, address)
                .unwrap();
        }
        let peer = peers_db.get_peer_by_node_id(&node_peers[11].node_id).unwrap().unwrap();
        assert!(!peer.all_addresses_failed());

        // Test 'set_metadata'
        peers_db
            .set_metadata(&node_peers[5].node_id, 111, vec![1, 2, 3])
            .unwrap();
        peers_db
            .set_metadata(&node_peers[5].node_id, 222, vec![4, 5, 6])
            .unwrap();
        let peer = peers_db.get_peer_by_node_id(&node_peers[5].node_id).unwrap().unwrap();
        assert_eq!(peer.metadata.get(&111).unwrap(), &[1, 2, 3]);
        assert_eq!(peer.metadata.get(&222).unwrap(), &[4, 5, 6]);
    }

    fn mark_failed(peer: &mut Peer) {
        let addresses = peer.addresses.address_iter().cloned().collect::<Vec<_>>();
        for addr in addresses {
            peer.addresses
                .mark_failed_connection_attempt(&addr, "Misbehave".to_string());
        }
    }

    fn reset_all_stats(peer: &mut Peer) {
        let addresses = peer.addresses.address_iter().cloned().collect::<Vec<_>>();
        for addr in &addresses {
            peer.addresses.reset_stats_to_default(addr);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_delete_all_stale_peers() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node peers
        let mut node_peers = Vec::with_capacity(30);
        for _i in 0..30 {
            let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            node_peers.push(peer.clone());
        }
        node_peers.sort_by_key(|p| p.node_id.distance(&peers_db.this_peer_identity.node_id));
        // Mark seed peers (0, 8, 16, 24)
        node_peers.iter_mut().enumerate().for_each(|(i, peer)| {
            if i % 8 == 0 {
                peer.flags = PeerFlags::SEED;
            }
        });
        let original_seeds = node_peers
            .iter()
            .filter(|&p| (p.flags == PeerFlags::SEED))
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        // Mark node peers failed (7, 13, 22, 23, 24, 25)
        for i in [7, 13, 22, 23, 24, 25] {
            mark_failed(&mut node_peers[i]);
        }
        let original_failed_nodes = node_peers
            .iter()
            .filter(|&p| p.all_addresses_failed())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        // Set never seen - node peers (14, 15, 16, 17, 18, 19, 20, 21)
        let original_never_seen_nodes = node_peers[14..=21]
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        for peer in node_peers.iter_mut().take(21 + 1).skip(14) {
            reset_all_stats(peer);
        }
        // Shuffle and add to db
        let mut shuffled = node_peers.clone();
        shuffled.shuffle(&mut OsRng);
        shuffled.iter().for_each(|peer| {
            peers_db.add_or_update_peer(peer.clone()).unwrap();
        });

        // Create new wallet peers
        let mut wallet_peers = Vec::with_capacity(30);
        for _i in 0..30 {
            let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT);
            wallet_peers.push(peer.clone());
        }
        wallet_peers.sort_by_key(|p| p.node_id.distance(&peers_db.this_peer_identity.node_id));
        // Mark wallet peers failed (7, 13, 22, 23, 24, 25)
        for i in [7, 13, 22, 23, 24, 25] {
            mark_failed(&mut wallet_peers[i]);
        }
        let original_failed_wallets = wallet_peers
            .iter()
            .filter(|&p| p.all_addresses_failed())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        // Set never seen - node peers (14, 15, 16, 17, 18, 19, 20, 21)
        let original_never_seen_wallets = wallet_peers[14..=21]
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        for peer in wallet_peers.iter_mut().take(21 + 1).skip(14) {
            reset_all_stats(peer);
        }
        // Shuffle and add to db
        let mut shuffled = wallet_peers.clone();
        shuffled.shuffle(&mut OsRng);
        shuffled.iter().for_each(|peer| {
            peers_db.add_or_update_peer(peer.clone()).unwrap();
        });

        let original_peers = node_peers
            .iter()
            .chain(wallet_peers.iter())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();

        // Set deleted - node and wallet peers (1, 2, 7)
        let original_deleted_nodes = vec![
            node_peers[1].node_id.clone(),
            node_peers[2].node_id.clone(),
            node_peers[7].node_id.clone(),
        ];
        original_deleted_nodes.iter().for_each(|node_id| {
            peers_db.set_deleted_at(node_id).unwrap();
        });
        let original_deleted_wallets = vec![
            wallet_peers[1].node_id.clone(),
            wallet_peers[2].node_id.clone(),
            wallet_peers[7].node_id.clone(),
        ];
        original_deleted_wallets.iter().for_each(|node_id| {
            peers_db.set_deleted_at(node_id).unwrap();
        });

        // Set banned - node and wallet peers (5, 6, 7)
        let original_banned_nodes = node_peers[5..=7].iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        let original_banned_wallets = wallet_peers[5..=7]
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        for peer in original_banned_nodes.iter().chain(original_banned_wallets.iter()) {
            peers_db
                .set_banned(peer, Duration::from_secs(12345), "Misbehaviour is punished".to_string())
                .unwrap();
        }

        // Set inactive - node peers (8, 9)
        let last_seen = chrono::Utc::now().naive_utc() -
            chrono::Duration::from_std(Duration::from_secs(120)).unwrap_or(TimeDelta::MAX);
        let original_inactive_nodes = node_peers[8..=9].iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        for peer in &original_inactive_nodes {
            for address in node_peers
                .iter()
                .find(|p| peer == &p.node_id)
                .unwrap()
                .addresses
                .addresses()
            {
                peers_db.set_last_seen(peer, last_seen, address.address()).unwrap();
            }
        }
        // Set inactive - wallet peers (0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 26, 27, 28, 29)
        let original_inactive_wallets = wallet_peers[0..=4]
            .iter()
            .chain(wallet_peers[8..=12].iter().chain(wallet_peers[26..=29].iter()))
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        for peer in &original_inactive_wallets {
            for address in wallet_peers
                .iter()
                .find(|p| peer == &p.node_id)
                .unwrap()
                .addresses
                .addresses()
            {
                peers_db.set_last_seen(peer, last_seen, address.address()).unwrap();
            }
        }

        // - build verification data (all)
        //   - seed peers
        let seed_peers_ids = peers_db
            .get_seed_peers()
            .unwrap()
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(original_seeds.iter().all(|v| seed_peers_ids.contains(v)));
        //   - all peers
        let all_peers = peers_db.get_all_peers(None).unwrap();
        let all_peers_ids = all_peers.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        assert!(original_peers.iter().all(|p| all_peers_ids.contains(p)));

        // - build verification data (nodes)
        let nodes_from_db = all_peers.iter().filter(|p| p.features.is_node()).collect::<Vec<_>>();
        //   - failed nodes
        let mut nodes_failed = nodes_from_db
            .iter()
            .filter(|p| p.all_addresses_failed())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(nodes_failed.iter().all(|p| original_failed_nodes.contains(p)));
        nodes_failed.retain(|p| !seed_peers_ids.contains(p));
        //   - banned nodes
        let mut nodes_banned = nodes_from_db
            .iter()
            .filter(|p| p.is_banned())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(nodes_banned.iter().all(|p| original_banned_nodes.contains(p)));
        nodes_banned.retain(|p| !nodes_failed.contains(p));
        //   - inactive nodes
        let stale_time_cutoff = chrono::Utc::now().naive_utc() -
            chrono::Duration::from_std(Duration::from_secs(60)).unwrap_or(TimeDelta::MAX);
        let mut nodes_inactive = nodes_from_db
            .iter()
            .filter(|p| p.last_seen().is_some() && p.last_seen().unwrap() < stale_time_cutoff)
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(nodes_inactive.iter().all(|p| original_inactive_nodes.contains(p)));
        nodes_inactive.retain(|p| !seed_peers_ids.contains(p));
        nodes_inactive.retain(|p| !nodes_banned.contains(p));
        //   - deleted nodes
        let nodes_deleted = nodes_from_db
            .iter()
            .filter(|p| p.deleted_at.is_some())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(nodes_deleted.iter().all(|p| original_deleted_nodes.contains(p)));
        //    - never seen before nodes
        let mut nodes_never_seen = nodes_from_db
            .iter()
            .filter(|p| p.last_seen().is_none())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(nodes_never_seen.iter().all(|p| original_never_seen_nodes.contains(p)));
        nodes_never_seen.retain(|p| !seed_peers_ids.contains(p));

        // - build verification data (wallets)
        let wallets_from_db = all_peers.iter().filter(|p| p.features.is_client()).collect::<Vec<_>>();
        //   - failed wallets
        let wallets_failed = wallets_from_db
            .iter()
            .filter(|p| p.all_addresses_failed())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(wallets_failed.iter().all(|p| original_failed_wallets.contains(p)));
        //   - banned wallets
        let mut wallets_banned = wallets_from_db
            .iter()
            .filter(|p| p.is_banned())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(wallets_banned.iter().all(|p| original_banned_wallets.contains(p)));
        wallets_banned.retain(|p| !wallets_failed.contains(p));
        //   - inactive wallets
        let wallets_inactive = wallets_from_db
            .iter()
            .filter(|p| p.last_seen().is_some() && p.last_seen().unwrap() < stale_time_cutoff)
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(wallets_inactive.iter().all(|p| original_inactive_wallets.contains(p)));
        //   - deleted wallets
        let wallets_deleted = wallets_from_db
            .iter()
            .filter(|&p| p.deleted_at.is_some())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(wallets_deleted.iter().all(|p| original_deleted_wallets.contains(p)));
        //    - never seen before wallets
        let wallets_never_seen = wallets_from_db
            .iter()
            .filter(|p| p.last_seen().is_none())
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert!(wallets_never_seen
            .iter()
            .all(|p| original_never_seen_wallets.contains(p)));

        // - perform test
        const NEIGHBOUR_COUNT: usize = 17;
        let stale_peers_deleted = peers_db
            .delete_all_stale_peers(Duration::from_secs(60), NEIGHBOUR_COUNT)
            .unwrap();
        assert_eq!(stale_peers_deleted.len(), 21);

        // - verify nodes
        //   - seed peers (0, 8, 16, 24)
        //   - node peers failed (7, 13, 22, 23, 24, 25)
        //   - deleted node peers (1, 2, 7)
        //   - banned node peers (5, 6, 7)
        //   - inactive node peers (8, 9)
        //   - never seen node peers (14, 15, 16, 17, 18, 19, 20, 21)
        let remaining = peers_db.get_all_peers(None).unwrap();
        assert_eq!(remaining.len(), 39);
        let mut remaining_nodes = peers_db.get_all_peers(Some(PeerFeatures::COMMUNICATION_NODE)).unwrap();
        remaining_nodes.sort_by_key(|p| p.node_id.distance(&peers_db.this_peer_identity.node_id));
        let remaining_nodes_ids = remaining_nodes.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        assert_eq!(remaining_nodes.len(), 22);
        // - verify all seeds are still present
        assert!(seed_peers_ids.iter().all(|p| remaining_nodes_ids.contains(p)));
        // - verify all banned nodes (that were not delted) are still present
        assert!(nodes_banned.iter().all(|p| remaining_nodes_ids.contains(p)));
        // - verify deleted nodes are removed
        assert!(!remaining_nodes_ids.iter().any(|p| nodes_deleted.contains(p)));
        // - verify failed nodes are removed
        assert!(!remaining_nodes_ids.iter().any(|p| nodes_failed.contains(p)));
        // - verify not seen recently nodes are removed
        assert!(!remaining_nodes_ids.iter().any(|p| nodes_inactive.contains(p)));
        // - verify never seen nodes are NOT removed
        assert!(nodes_never_seen.iter().all(|p| remaining_nodes_ids.contains(p)));

        // - verify wallets
        //   - wallet peers failed (7, 13, 22, 23, 24, 25)
        //   - deleted wallet peers (1, 2, 7)
        //   - banned wallet peers (5, 6, 7)
        //   - inactive wallet peers (0, 1, 2, 3, 4, 8, 9, 10, 11, 12, 26, 27, 28, 29)
        //   - never seen wallet peers (14, 15, 16, 17, 18, 19, 20, 21)
        let mut remaining_wallets = peers_db
            .get_all_peers(Some(PeerFeatures::COMMUNICATION_CLIENT))
            .unwrap();
        remaining_wallets.sort_by_key(|p| p.node_id.distance(&peers_db.this_peer_identity.node_id));
        let remaining_wallets_ids = remaining_wallets.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        assert_eq!(remaining_wallets.len(), NEIGHBOUR_COUNT);
        // - verify deleted wallets are removed
        assert!(!remaining_wallets_ids.iter().any(|p| wallets_deleted.contains(p)));

        let debug_print_results = false;
        if debug_print_results {
            println!();
            println!("original_seeds:              {:?}", original_seeds);
            println!("original_failed_nodes:       {:?}", original_failed_nodes);
            println!("original_never_seen_nodes:   {:?}", original_never_seen_nodes);
            println!("original_failed_wallets:     {:?}", original_failed_wallets);
            println!("original_never_seen_wallets: {:?}", original_never_seen_wallets);
            println!("original_deleted_nodes:      {:?}", original_deleted_nodes);
            println!("original_deleted_wallets:    {:?}", original_deleted_wallets);
            println!("original_banned_nodes:       {:?}", original_banned_nodes);
            println!("original_banned_wallets:     {:?}", original_banned_wallets);
            println!("original_inactive_nodes:     {:?}", original_inactive_nodes);
            println!("original_inactive_wallets:   {:?}", original_inactive_wallets);

            println!();
            println!("stale_peers_deleted:         {}", stale_peers_deleted.len());
            println!("stale_peers_deleted:         {:?}", stale_peers_deleted);
            println!("remaining:                   {}", remaining.len());
            println!("remaining_nodes:             {}", remaining_nodes_ids.len());
            println!("remaining_wallets:           {}", remaining_wallets_ids.len());

            println!();
            println!("remaining nodes");
            for (i, peer) in remaining_nodes.iter().enumerate() {
                println!(
                    "{}: {}, seed: {}, offline: {}, banned: {}, deleted: {}, failed: {}, last seen: {}, inactive: {}",
                    i,
                    peer.node_id.to_hex(),
                    peer.is_seed(),
                    peer.is_offline(),
                    peer.is_banned(),
                    peer.deleted_at.is_some(),
                    peer.all_addresses_failed(),
                    peer.last_seen().is_some(),
                    if let Some(last_seen) = peer.last_seen() {
                        (last_seen < stale_time_cutoff).to_string()
                    } else {
                        "n/a".to_string()
                    },
                );
            }

            println!();
            println!("remaining wallets");
            for (i, peer) in remaining_wallets.iter().enumerate() {
                println!(
                    "{}: {}, seed: {}, offline: {}, banned: {}, deleted: {}, failed: {}, last seen: {}, inactive: {}",
                    i,
                    peer.node_id.to_hex(),
                    peer.is_seed(),
                    peer.is_offline(),
                    peer.is_banned(),
                    peer.deleted_at.is_some(),
                    peer.all_addresses_failed(),
                    peer.last_seen().is_some(),
                    if let Some(last_seen) = peer.last_seen() {
                        (last_seen < stale_time_cutoff).to_string()
                    } else {
                        "n/a".to_string()
                    },
                );
            }
        }
    }

    #[test]
    fn test_get_closest_n_good_standing_peer_node_ids() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node peers
        let mut node_peers = Vec::with_capacity(20);
        for i in 0..20 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            if i % 4 == 0 {
                peer.ban_for(Duration::from_secs(3600), "Test ban".to_string());
            }
            if i % 5 == 0 {
                peer.deleted_at = Some(chrono::Utc::now().naive_utc());
            }
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }
        // Create new wallet peers
        let mut wallet_peers = Vec::with_capacity(20);
        for i in 0..20 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT);
            if i % 4 == 0 {
                peer.ban_for(Duration::from_secs(3600), "Test ban".to_string());
            }
            if i % 5 == 0 {
                peer.deleted_at = Some(chrono::Utc::now().naive_utc());
            }
            wallet_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }
        // Mark some peers as failed
        for address in node_peers[6].addresses.addresses() {
            peers_db
                .set_last_failed_reason(
                    &node_peers[6].node_id,
                    "Connection failed".to_string(),
                    address.address(),
                )
                .unwrap();
        }
        for address in wallet_peers[6].addresses.addresses() {
            peers_db
                .set_last_failed_reason(
                    &wallet_peers[6].node_id,
                    "Connection failed".to_string(),
                    address.address(),
                )
                .unwrap();
        }

        // Test the function
        let closest_peers = peers_db
            .get_closest_n_good_standing_peer_node_ids(10, PeerFeatures::COMMUNICATION_NODE)
            .unwrap();

        // Verify the results
        assert_eq!(closest_peers.len(), 10);
        for node_id in &closest_peers {
            let peer = peers_db.get_peer_by_node_id(node_id).unwrap().unwrap();
            assert!(!peer.is_banned());
            assert!(peer.deleted_at.is_none());
            assert!(!peer.all_addresses_failed());
        }

        // Verify sorting by distance
        let region_node_id = peers_db.this_peer_identity().node_id;
        for i in 0..closest_peers.len() - 1 {
            assert!(closest_peers[i].distance(&region_node_id) <= closest_peers[i + 1].distance(&region_node_id));
        }
    }

    #[test]
    fn test_duration_to_i64_ms_infallible() {
        // None input should yield None
        assert_eq!(duration_to_i64_ms_infallible(None), None);

        // ms
        assert_eq!(duration_to_i64_ms_infallible(Some(Duration::from_millis(0))), Some(0));
        assert_eq!(duration_to_i64_ms_infallible(Some(Duration::from_millis(42))), Some(42));
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_millis(1234))),
            Some(1234)
        );

        // s
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_secs(12))),
            Some(12 * 1000)
        );
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_secs(1234))),
            Some(1234 * 1000)
        );

        // d
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_secs(3 * 60 * 60 * 24))),
            Some(3 * 60 * 60 * 24 * 1000)
        );
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_secs(123 * 60 * 60 * 24))),
            Some(123 * 60 * 60 * 24 * 1000)
        );

        // max
        assert_eq!(
            duration_to_i64_ms_infallible(Some(Duration::from_secs(u64::MAX))),
            Some(i64::MAX)
        );
    }

    #[test]
    fn test_u32_to_i32_infallible() {
        // ms
        assert_eq!(u32_to_i32_infallible(0u32), 0i32);
        assert_eq!(u32_to_i32_infallible(1234u32), 1234i32);
        assert_eq!(u32_to_i32_infallible(u32::MAX), i32::MAX);
    }
}
