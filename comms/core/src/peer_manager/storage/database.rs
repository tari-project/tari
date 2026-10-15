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
    ExpressionMethods,
    QueryDsl,
    RunQueryDsl,
    SqliteConnection,
    dsl::not,
    prelude::*,
    r2d2::{ConnectionManager, PooledConnection},
};
use diesel_migrations::{EmbeddedMigrations, embed_migrations};
use log::{trace, warn};
use multiaddr::Multiaddr;
use tari_common_sqlite::{connection::DbConnection, error::StorageError};
use tari_utilities::{hex, hex::Hex};

#[cfg(feature = "metrics")]
use crate::peer_manager::metrics;
use crate::{
    net_address::{
        MultiaddrWithStats,
        MultiaddressesWithStats,
        PeerAddressSource,
        address_netgroup,
        is_external_address,
    },
    peer_manager::{
        NodeId,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerId,
        generate_peer_id_as_i64,
        peer_id::peer_id_from_i64,
        storage::schema::{db_metadata, multi_addresses, node_identity, peers},
    },
    peer_validator::validate_address_shape,
    protocol::ProtocolId,
    types::{CommsPublicKey, TransportProtocol},
    utils::{
        datetime::{clamp_to_max_stored_datetime, max_stored_datetime, safe_future_datetime_from_duration},
        text::sanitize_for_display,
    },
};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/peer_manager/storage/migrations");
const LOG_TARGET: &str = "comms::peer_manager::storage::db";

/// How recently a peer must have been seen to still count as "known good" when selecting random
/// peers. Peers are gossiped far faster than they are re-verified, so without a recency bound the
/// candidate set is dominated by peers that were reachable once and have long since gone.
const KNOWN_GOOD_LAST_SEEN_WINDOW: TimeDelta = TimeDelta::hours(1);

/// `db_metadata` key holding the [`ADDRESS_CLASSIFIER_VERSION`] this database was last classified
/// with.
const ADDRESS_CLASSIFIER_VERSION_KEY: &str = "address_classifier_version";
/// Bump this whenever [`is_external_address`] changes, so that stored classifications are recomputed
/// once on the next start.
const ADDRESS_CLASSIFIER_VERSION: u32 = 1;

/// `db_metadata` key holding the [`ADDRESS_BYTES_VERSION`] whose full pass over the stored addresses has run.
const ADDRESS_BYTES_VERSION_KEY: &str = "address_bytes_version";
/// Bump this to run [`PeerDatabaseSql::backfill_address_bytes`] over every stored address again.
/// Version 2 also removes rows whose address is not well formed. Version 3 also removes rows whose stored claim holds
/// an address that is not well formed, and recomputes `netgroup` (DNS names are grouped by their last two labels).
const ADDRESS_BYTES_VERSION: u32 = 3;

/// The most rows peer garbage collection deletes in one write transaction, so that it never holds the database's single
/// writer for long.
const GARBAGE_COLLECTION_BATCH_SIZE: usize = 500;
/// How many garbage collection candidates one plain read fetches, to be deleted in batches.
const GARBAGE_COLLECTION_READ_SIZE: i64 = 5000;
/// How long a soft-deleted peer is kept before garbage collection removes it for good.
const SOFT_DELETED_PEER_RETENTION: TimeDelta = TimeDelta::hours(24);

/// What one run of [`PeerDatabaseSql::garbage_collect`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GarbageCollectionStats {
    /// Never-seen peers removed to bring the table back under the row cap
    pub excess_peers_removed: usize,
    /// Peers with addresses and an active ban removed to bring them back under their cap
    pub banned_peers_removed: usize,
    /// Peers without any address (typically only carrying a handshake ban) removed to bring them back under their cap
    pub address_less_peers_removed: usize,
    /// Soft-deleted peers removed after their retention period
    pub soft_deleted_peers_removed: usize,
}

/// The passes of [`PeerDatabaseSql::garbage_collect`]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GarbageCollectionPass {
    /// Live peers with addresses and no active ban, none of them reached recently or configured
    NeverSeen,
    /// Live peers with addresses and an active ban
    Banned,
    /// Live peers without any address
    AddressLess,
    /// Peers soft deleted before the retention period
    SoftDeleted,
}

/// What every garbage collection pass needs to decide eligibility
struct GarbageCollectionContext {
    now: NaiveDateTime,
    deleted_before: NaiveDateTime,
    /// An address last seen before this no longer counts as reached
    seen_since: NaiveDateTime,
    config_source: String,
    connected_hex: Vec<String>,
}

/// The peers `pass` may remove. Used to count them, to find candidates and to re-check candidates right before they
/// are deleted. Connected peers and seed peers are never eligible.
fn garbage_collection_eligible<'a>(
    pass: GarbageCollectionPass,
    context: &'a GarbageCollectionContext,
) -> peers::BoxedQuery<'a, diesel::sqlite::Sqlite> {
    let not_seed =
        diesel::dsl::sql::<diesel::sql_types::Bool>(&format!("peers.flags & {} = 0", PeerFlags::SEED.to_i32()));
    let has_address = diesel::dsl::exists(multi_addresses::table.filter(multi_addresses::peer_id.eq(peers::peer_id)));
    let has_configured_address = diesel::dsl::exists(
        multi_addresses::table
            .filter(multi_addresses::peer_id.eq(peers::peer_id))
            .filter(multi_addresses::source.eq(&context.config_source)),
    );
    let query = peers::table
        .filter(peers::node_id.ne_all(&context.connected_hex))
        .into_boxed();
    match pass {
        GarbageCollectionPass::NeverSeen => {
            // Peers reached recently are kept; a reach older than `seen_since` no longer counts
            let reached = diesel::dsl::exists(
                multi_addresses::table
                    .filter(multi_addresses::peer_id.eq(peers::peer_id))
                    .filter(multi_addresses::last_seen.ge(context.seen_since)),
            );
            // An expired ban is no ban: such a peer is judged like any other
            query
                .filter(peers::deleted_at.is_null())
                .filter(peers::banned_until.is_null().or(peers::banned_until.le(context.now)))
                .filter(has_address)
                .filter(not(reached))
                .filter(not(has_configured_address))
                .filter(not_seed)
        },
        // Active bans only: `banned_until` is not cleared when a ban expires
        GarbageCollectionPass::Banned => query
            .filter(peers::deleted_at.is_null())
            .filter(peers::banned_until.gt(context.now))
            .filter(has_address)
            .filter(not(has_configured_address))
            .filter(not_seed),
        GarbageCollectionPass::AddressLess => query
            .filter(peers::deleted_at.is_null())
            .filter(not(has_address))
            .filter(not_seed),
        GarbageCollectionPass::SoftDeleted => query
            .filter(peers::deleted_at.lt(context.deleted_before))
            .filter(peers::banned_until.is_null().or(peers::banned_until.le(context.now))),
    }
}

/// The peer ids eligible for removal in `pass`, in removal order.
fn garbage_collection_candidates<'a>(
    pass: GarbageCollectionPass,
    context: &'a GarbageCollectionContext,
) -> peers::BoxedQuery<'a, diesel::sqlite::Sqlite, diesel::sql_types::BigInt> {
    let query = garbage_collection_eligible(pass, context).select(peers::peer_id);
    match pass {
        // Random, so that whoever floods the table cannot choose which honest peers go
        GarbageCollectionPass::NeverSeen => query.order_by(diesel::dsl::sql::<diesel::sql_types::Integer>("RANDOM()")),
        // Soonest-ending ban first (for address-less rows: no ban and expired bans first); oldest first among equals
        GarbageCollectionPass::Banned | GarbageCollectionPass::AddressLess => {
            query.order_by((peers::banned_until.asc(), peers::added_at.asc()))
        },
        GarbageCollectionPass::SoftDeleted => query.order_by(peers::peer_id.asc()),
    }
}

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
        // Must run first: every later read decodes `address_bytes`
        PeerDatabaseSql::backfill_address_bytes(&instance)?;
        PeerDatabaseSql::reclassify_stored_address_externality(&instance)?;
        PeerDatabaseSql::clamp_stored_ban_times(&instance)?;
        Ok(instance)
    }

    /// Fills in `address_bytes` and `netgroup` for addresses stored as text, and removes the rows whose text cannot be
    /// decoded or whose claimed address is not well formed.
    ///
    /// The text form of a multiaddr is not lossless, and an address whose DNS name contains `/` does not parse back at
    /// all - before `address_bytes` existed, one such row made every peer query fail. Those rows cannot be repaired, so
    /// they are deleted. The same applies to a row whose `source` (which contains the full signed claim) does not
    /// decode.
    ///
    /// The first pass covers every row and is recorded in `db_metadata` under [`ADDRESS_BYTES_VERSION_KEY`]. Later
    /// starts only visit rows that have no bytes, which can only have been written by an older version after a
    /// downgrade, so the step is idempotent and cheap once done.
    fn backfill_address_bytes(&self) -> Result<(), StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let applied_version = db_metadata::table
                .filter(db_metadata::key.eq(ADDRESS_BYTES_VERSION_KEY))
                .select(db_metadata::value)
                .first::<String>(conn)
                .optional()?
                .and_then(|version| version.parse::<u32>().ok());
            let full_pass = applied_version != Some(ADDRESS_BYTES_VERSION);

            let mut query = multi_addresses::table
                .select((
                    multi_addresses::address_id,
                    multi_addresses::address,
                    multi_addresses::source,
                ))
                .into_boxed();
            if !full_pass {
                query = query.filter(multi_addresses::address_bytes.eq(Vec::<u8>::new()));
            }
            let stored = query.load::<(Option<i32>, String, String)>(conn)?;

            let mut removed = 0usize;
            for (address_id, address_text, source) in stored {
                let Some(address_id) = address_id else {
                    continue;
                };
                // The one place stored text is parsed: it is all an older version wrote
                let address = Multiaddr::from_str(&address_text).ok().filter(|a| !a.is_empty());
                let source = serde_json::from_str::<PeerAddressSource>(&source).ok();
                let row = multi_addresses::table.filter(multi_addresses::address_id.eq(address_id));
                // A claimed address that parses but is not well formed was stored before claims were checked for
                // it, e.g. the text `/dns4/a/tcp/80/ip4/10.0.0.1/tcp/1` of a DNS name containing `/`: it now reads
                // as a different address than the one signed, and serving it on to other peers would be blamed on
                // us. The same goes for the stored claim: it is served whole, so one bad address in it (even one we
                // never stored as a row) makes the row unsafe to keep. Configured addresses are the operator's and
                // are kept.
                let well_formed = |address: &Multiaddr, source: &PeerAddressSource| {
                    if source.is_config() {
                        return true;
                    }
                    let claim_is_well_formed = source.peer_identity_claim().is_none_or(|claim| {
                        claim
                            .addresses
                            .iter()
                            .all(|claimed| validate_address_shape(claimed).is_ok())
                    });
                    validate_address_shape(address).is_ok() && claim_is_well_formed
                };
                // The source is left as it is, in the text form older versions read
                match (address, source) {
                    (Some(address), Some(source)) if well_formed(&address, &source) => {
                        diesel::update(row)
                            .set((
                                multi_addresses::address_bytes.eq(address.to_vec()),
                                multi_addresses::netgroup.eq(address_netgroup(&address)),
                            ))
                            .execute(conn)?;
                    },
                    _ => {
                        warn!(
                            target: LOG_TARGET,
                            "Removing stored peer address that cannot be decoded: {address_text:?}"
                        );
                        diesel::delete(row).execute(conn)?;
                        removed = removed.saturating_add(1);
                    },
                }
            }

            if removed > 0 {
                warn!(
                    target: LOG_TARGET,
                    "Removed {removed} stored peer address(es) that could not be decoded"
                );
            }

            if full_pass {
                diesel::replace_into(db_metadata::table)
                    .values((
                        db_metadata::key.eq(ADDRESS_BYTES_VERSION_KEY),
                        db_metadata::value.eq(ADDRESS_BYTES_VERSION.to_string()),
                    ))
                    .execute(conn)?;
            }

            Ok(())
        })
    }

    /// Rewrites ban times stored with a year past 9999 to [`max_stored_datetime`].
    ///
    /// Such a time (a ban "forever") is written with a leading `+` and compares as text before every current time, so
    /// it read as long expired: the peer stayed a dial candidate and garbage collection removed it, losing the ban.
    /// New writes are clamped; this repairs rows written before that, or by an older version. It is idempotent and
    /// touches no row once they are repaired.
    fn clamp_stored_ban_times(&self) -> Result<(), StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let repaired = diesel::update(peers::table.filter(peers::banned_until.is_not_null()).filter(
            diesel::dsl::sql::<diesel::sql_types::Bool>("peers.banned_until LIKE '+%'"),
        ))
        .set(peers::banned_until.eq(max_stored_datetime()))
        .execute(&mut conn)?;
        if repaired > 0 {
            warn!(
                target: LOG_TARGET,
                "Repaired {repaired} stored indefinite peer ban(s) that read as expired"
            );
        }
        Ok(())
    }

    /// Recomputes the persisted `is_external` flag for every stored address, once per classifier
    /// version.
    ///
    /// The flag is derived from the address itself, so widening the classifier leaves existing rows
    /// stale. Rows are only rewritten by the normal update path when a peer publishes a strictly
    /// newer signed claim, and an address that the peer validator now filters out is never handed to
    /// that path again - so without this pass a local address stored by an older version would keep
    /// `is_external = true` indefinitely, and keep being served to other nodes by the queries that
    /// filter on it.
    ///
    /// This is a data migration rather than startup work: the classifier lives in Rust, so it cannot
    /// run from a migration's `up.sql` the way `2025-07-19-085200_external_flag` did. Once the pass
    /// has run, the applied [`ADDRESS_CLASSIFIER_VERSION`] is recorded and later starts do nothing
    /// beyond reading it back.
    fn reclassify_stored_address_externality(&self) -> Result<(), StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let applied_version = db_metadata::table
                .filter(db_metadata::key.eq(ADDRESS_CLASSIFIER_VERSION_KEY))
                .select(db_metadata::value)
                .first::<String>(conn)
                .optional()?
                .and_then(|version| version.parse::<u32>().ok());
            if applied_version == Some(ADDRESS_CLASSIFIER_VERSION) {
                return Ok(());
            }

            let stored = multi_addresses::table
                .select((
                    multi_addresses::address_id,
                    multi_addresses::address,
                    multi_addresses::address_bytes,
                    multi_addresses::is_external,
                ))
                .load::<(Option<i32>, String, Vec<u8>, bool)>(conn)?;

            let mut reclassified = 0usize;
            for (address_id, address, address_bytes, was_external) in stored {
                let Some(address_id) = address_id else {
                    continue;
                };
                // An address we can no longer decode can never be dialled, so it is not external.
                let is_external = Multiaddr::try_from(address_bytes).is_ok_and(|addr| is_external_address(&addr));
                if is_external == was_external {
                    continue;
                }

                diesel::update(multi_addresses::table.filter(multi_addresses::address_id.eq(address_id)))
                    .set(multi_addresses::is_external.eq(is_external))
                    .execute(conn)?;
                trace!(
                    target: LOG_TARGET,
                    "Reclassified stored address '{address}' as {}external",
                    if is_external { "" } else { "not " }
                );
                reclassified = reclassified.saturating_add(1);
            }

            if reclassified > 0 {
                warn!(
                    target: LOG_TARGET,
                    "Reclassified {reclassified} stored peer address(es) against address classifier \
                     v{ADDRESS_CLASSIFIER_VERSION}"
                );
            }

            diesel::replace_into(db_metadata::table)
                .values((
                    db_metadata::key.eq(ADDRESS_CLASSIFIER_VERSION_KEY),
                    db_metadata::value.eq(ADDRESS_CLASSIFIER_VERSION.to_string()),
                ))
                .execute(conn)?;

            Ok(())
        })
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
                if self.this_peer_identity.public_key.to_hex() ==
                    node_identity_indexes.first().expect("already checked").public_key &&
                    self.this_peer_identity.node_id.to_hex() ==
                        node_identity_indexes.first().expect("already checked").node_id
                {
                    return Ok(());
                } else {
                    // Update the database with the current node identity
                    let old_row = node_identity_indexes.first().expect("already checked");
                    warn!(target: LOG_TARGET,
                        "Node ID mismatch detected: {} (from file) vs {} (from DB). \
                         This may occur if the base_node_id file was deleted without updating the database. \
                         Updating database identity to match the file.",
                        self.this_peer_identity.node_id.to_hex(),
                        old_row.node_id
                    );
                    let affected =
                        diesel::update(node_identity::table.filter(node_identity::node_id.eq(old_row.node_id.clone())))
                            .set((
                                node_identity::public_key.eq(self.this_peer_identity.public_key.to_hex()),
                                node_identity::node_id.eq(self.this_peer_identity.node_id.to_hex()),
                                node_identity::features.eq(self.this_peer_identity.features.to_i32()),
                            ))
                            .execute(conn)?;
                    if affected != 1 {
                        return Err(StorageError::UnexpectedResult(format!(
                            "Expected to update 1 node_identity row, updated {}",
                            affected
                        )));
                    }
                    return Ok(());
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

    /// Add a new peer or update an existing peer with its associated multi-addresses
    pub fn add_or_update_peer(&self, peer: Peer) -> Result<PeerId, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let node_id = peer.node_id.clone();

            match self.get_peer_by_node_id_inner(&node_id, conn)? {
                Some(mut existing_peer) => {
                    existing_peer.merge(&peer);
                    let update_peer_sql = PeerDatabaseSql::update_peer_sql(existing_peer.clone())?;
                    self.update_peer_inner(update_peer_sql, conn)?;
                    Ok(existing_peer.id.unwrap_or_default())
                },
                None => {
                    trace!(target: LOG_TARGET, "Adding peer with node id '{node_id}'");
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
                    peer.addresses.add_or_update_addresses(addresses, source);
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
            flags: peer.flags.to_i32(),
            banned_until: peer.banned_until.map(clamp_to_max_stored_datetime),
            banned_reason: peer.banned_reason.clone(),
            features: peer.features.to_i32(),
            supported_protocols: serialize_protocols(&peer.supported_protocols),
            added_at: peer.added_at,
            user_agent: peer.user_agent.clone(),
            metadata: serialize_metadata(&peer.metadata)?,
            deleted_at: peer.deleted_at,
        };

        let mut new_addresses_sql = Vec::with_capacity(peer.addresses.len());
        for address in peer.addresses.iter() {
            if !address.address().is_empty() && has_lossless_text(address.address()) {
                new_addresses_sql.push(NewMultiaddrWithStatsSql {
                    address_id: None, // This will be set automatically
                    peer_id: 0,       // This will be set automatically
                    address: address.address().to_string(),
                    is_external: address.is_external(),
                    last_seen: address.last_seen(),
                    connection_attempts: i32::try_from(address.connection_attempts())?,
                    avg_initial_dial_time: duration_to_i64_ms_infallible(address.avg_initial_dial_time()),
                    initial_dial_time_sample_count: i32::try_from(address.initial_dial_time_sample_count())?,
                    avg_latency: duration_to_i64_ms_infallible(address.avg_latency()),
                    latency_sample_count: i32::try_from(address.latency_sample_count())?,
                    last_attempted: address.last_attempted(),
                    last_failed_reason: address.last_failed_reason().map(|s| s.to_string()),
                    quality_score: address.quality_score(),
                    source: serde_json::to_string(&address.source())
                        .map_err(|err| StorageError::UnexpectedResult(err.to_string()))?,
                    address_bytes: address.address().to_vec(),
                    netgroup: address_netgroup(address.address()),
                });
            }
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
                "Could not insert peer '{node_id}'"
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
            node_id: Some(peer.node_id.to_hex()),
            flags: Some(peer.flags.to_i32()),
            banned_until: Some(peer.banned_until.map(clamp_to_max_stored_datetime)),
            banned_reason: Some(peer.banned_reason.clone()),
            features: Some(peer.features.to_i32()),
            supported_protocols: Some(serialize_protocols(&peer.supported_protocols)),
            user_agent: Some(peer.user_agent.clone()),
            metadata: Some(serialize_metadata(&peer.metadata)?),
            deleted_at: Some(peer.deleted_at),
        };

        let mut update_addresses_sql = Vec::with_capacity(peer.addresses.len());
        for address in peer.addresses.iter() {
            if !address.address().is_empty() && has_lossless_text(address.address()) {
                update_addresses_sql.push(UpdateMultiaddrWithStatsSql {
                    address: Some(address.address().to_string()),
                    is_external: Some(address.is_external()),
                    last_seen: Some(address.last_seen()),
                    connection_attempts: Some(i32::try_from(address.connection_attempts())?),
                    avg_initial_dial_time: Some(duration_to_i64_ms_infallible(address.avg_initial_dial_time())),
                    initial_dial_time_sample_count: Some(i32::try_from(address.initial_dial_time_sample_count())?),
                    avg_latency: Some(duration_to_i64_ms_infallible(address.avg_latency())),
                    latency_sample_count: Some(i32::try_from(address.latency_sample_count())?),
                    last_attempted: Some(address.last_attempted()),
                    last_failed_reason: Some(address.last_failed_reason().map(|s| s.to_string())),
                    quality_score: Some(address.quality_score()),
                    source: Some(
                        serde_json::to_string(&address.source())
                            .map_err(|err| StorageError::UnexpectedResult(err.to_string()))?,
                    ),
                    address_bytes: Some(address.address().to_vec()),
                    netgroup: Some(address_netgroup(address.address())),
                });
            }
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
        diesel::update(
            peers::table.filter(peers::node_id.eq(update_peer_sql.peer.node_id.clone().unwrap_or_default())),
        )
        .set(&update_peer_sql.peer)
        .execute(conn)?;
        let peer_id = peers::table
            .filter(peers::node_id.eq(update_peer_sql.peer.node_id.clone().unwrap_or_default()))
            .select(peers::peer_id)
            .first::<i64>(conn)?;

        // Update the associated multi-addresses
        // - Remove all stale addresses that are not in the update list
        // Addresses are matched on their bytes: the text form is not lossless.
        let final_addr_list: Vec<Vec<u8>> = update_peer_sql
            .addresses
            .iter()
            .filter_map(|a| a.address_bytes.clone())
            .collect();

        diesel::delete(
            multi_addresses::table
                .filter(multi_addresses::peer_id.eq(peer_id))
                .filter(not(multi_addresses::address_bytes.eq_any(&final_addr_list))),
        )
        .execute(conn)?;
        // - Update existing addresses and add new ones
        for address_update in update_peer_sql.addresses {
            let updated = diesel::update(
                multi_addresses::table
                    .filter(multi_addresses::address_bytes.eq(address_update.address_bytes.clone().unwrap_or_default()))
                    .filter(multi_addresses::peer_id.eq(peer_id)),
            )
            .set(&address_update)
            .execute(conn)?;
            // If the address does not exist, add it
            if updated == 0 {
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
    pub fn soft_delete_peer(&self, node_id: &NodeId) -> Result<Option<NodeId>, StorageError> {
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

    /// Set the banned metadata for a peer, returning 'Some(node_id)' if successful, 'None' otherwise. A ban never
    /// shortens an existing one: if the peer is already banned until later than this ban would last, the existing ban
    /// (and its reason) is kept. Use `reset_banned` to lift a ban.
    pub fn set_banned(
        &self,
        node_id: &NodeId,
        ban_duration: Duration,
        banned_reason: String,
    ) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        let banned_reason = sanitize_ban_reason(&banned_reason);
        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let banned_until =
                clamp_to_max_stored_datetime(safe_future_datetime_from_duration(ban_duration).naive_utc());

            let existing = peers::table
                .filter(peers::node_id.eq(node_id.to_string()))
                .select(peers::banned_until)
                .first::<Option<NaiveDateTime>>(conn)
                .optional()?;
            let Some(existing) = existing else {
                return Ok(None);
            };
            // An indefinite ban is stored as the latest representable time, so it is never replaced
            if existing.is_some_and(|existing| existing >= banned_until) {
                return Ok(Some(node_id.clone()));
            }

            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set((
                    peers::banned_until.eq(banned_until),
                    peers::banned_reason.eq(banned_reason.clone()),
                ))
                .execute(conn)?;
            if affected > 0 {
                Ok(Some(node_id.clone()))
            } else {
                Ok(None)
            }
        })
    }

    /// Ban the peer with this public key, adding a minimal peer row for it first if it is not stored yet.
    ///
    /// Peers are often banned during the connection handshake, before they have been stored, and a ban that only
    /// updated an existing row was lost for them. The row added here has no addresses and no features, so it is never a
    /// dial candidate; it exists only to carry the ban. Such rows are capped separately by garbage collection.
    ///
    /// As with [`Self::set_banned`], a stored ban that ends later is kept, with its reason.
    pub fn ban_or_add_banned_peer(
        &self,
        public_key: &CommsPublicKey,
        ban_duration: Duration,
        banned_reason: String,
    ) -> Result<NodeId, StorageError> {
        let node_id = NodeId::from_public_key(public_key);
        let banned_reason = sanitize_ban_reason(&banned_reason);
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let banned_until =
                clamp_to_max_stored_datetime(safe_future_datetime_from_duration(ban_duration).naive_utc());
            let existing = peers::table
                .filter(peers::node_id.eq(node_id.to_hex()))
                .select(peers::banned_until)
                .first::<Option<NaiveDateTime>>(conn)
                .optional()?;
            if existing.flatten().is_some_and(|existing| existing >= banned_until) {
                return Ok(node_id.clone());
            }
            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_hex())))
                .set((
                    peers::banned_until.eq(banned_until),
                    peers::banned_reason.eq(banned_reason.clone()),
                ))
                .execute(conn)?;
            if affected > 0 {
                return Ok(node_id.clone());
            }

            let mut peer = Peer::new(
                public_key.clone(),
                node_id.clone(),
                MultiaddressesWithStats::empty(),
                PeerFlags::empty(),
                PeerFeatures::NONE,
                Vec::new(),
                String::new(),
            );
            peer.banned_until = Some(banned_until);
            peer.banned_reason = banned_reason;
            let new_peer_sql = self.add_peer_sql(peer)?;
            self.add_peer_inner(new_peer_sql, conn)?;
            Ok(node_id.clone())
        })
    }

    /// Removes peers to keep each kind of peer row under its cap, then removes peers that were soft deleted more than
    /// [`SOFT_DELETED_PEER_RETENTION`] ago.
    ///
    /// Peer identities are free to create, and every inbound handshake stores one, so without caps the table grows
    /// without bound. Each cap counts only the rows that can be removed for it:
    /// - Peers with addresses and no active ban (an expired ban counts as none), none reached since `seen_since` and
    ///   none configured: at most `max_peer_rows`, removed in random order so that a flood cannot choose which honest
    ///   peers go.
    /// - Peers with addresses and an active ban: at most a fifth of `max_peer_rows`, the ban that ends soonest removed
    ///   first.
    /// - Peers without any address, mostly rows that only record a handshake ban: at most a fifth of `max_peer_rows`,
    ///   in the same order. The bans of removed rows are lost.
    ///
    /// Seed peers, peers with a configured address, recently reached peers and the peers in `connected` are never
    /// removed. Inbound-only peers are never marked as seen and wallets usually have no address, so without the
    /// `connected` exemption a long-lived connected neighbour would be removed, after which its messages are dropped as
    /// coming from an unknown peer and it can no longer be banned.
    ///
    /// Candidates are found with a plain read, which takes no write lock however large the table is. They are then
    /// deleted in chunks of [`GARBAGE_COLLECTION_BATCH_SIZE`], each in its own short write transaction that checks
    /// the chunk against the same conditions again (cheaply, by peer id), so a peer that was reached, banned or
    /// connected in the meantime is not removed.
    pub fn garbage_collect(
        &self,
        max_peer_rows: usize,
        connected: &[NodeId],
        seen_since: NaiveDateTime,
    ) -> Result<GarbageCollectionStats, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        let now = chrono::Utc::now().naive_utc();
        let context = GarbageCollectionContext {
            now,
            deleted_before: now
                .checked_sub_signed(SOFT_DELETED_PEER_RETENTION)
                .unwrap_or(NaiveDateTime::MIN),
            seen_since,
            // Matches the JSON stored in `multi_addresses.source` for `PeerAddressSource::Config`
            config_source: serde_json::to_string(&PeerAddressSource::Config)
                .map_err(|err| StorageError::UnexpectedResult(err.to_string()))?,
            connected_hex: connected.iter().map(|node_id| node_id.to_hex()).collect(),
        };
        let max_peer_rows = i64::try_from(max_peer_rows).unwrap_or(i64::MAX);
        let side_cap = max_peer_rows / 5;

        // Each cap counts only the rows its pass could remove. Counting rows that can never be removed (reached,
        // banned, connected, ...) would let a flood of such rows push out every removable honest peer on each run.
        let mut stats = GarbageCollectionStats::default();
        let excess = self
            .count_eligible(&mut conn, GarbageCollectionPass::NeverSeen, &context)?
            .saturating_sub(max_peer_rows);
        stats.excess_peers_removed =
            self.garbage_collection_pass(&mut conn, GarbageCollectionPass::NeverSeen, &context, Some(excess))?;
        let excess = self
            .count_eligible(&mut conn, GarbageCollectionPass::Banned, &context)?
            .saturating_sub(side_cap);
        stats.banned_peers_removed =
            self.garbage_collection_pass(&mut conn, GarbageCollectionPass::Banned, &context, Some(excess))?;
        let excess = self
            .count_eligible(&mut conn, GarbageCollectionPass::AddressLess, &context)?
            .saturating_sub(side_cap);
        stats.address_less_peers_removed =
            self.garbage_collection_pass(&mut conn, GarbageCollectionPass::AddressLess, &context, Some(excess))?;
        stats.soft_deleted_peers_removed =
            self.garbage_collection_pass(&mut conn, GarbageCollectionPass::SoftDeleted, &context, None)?;
        Ok(stats)
    }

    // A plain read: the number of peers `pass` may remove
    fn count_eligible(
        &self,
        conn: &mut SqliteConnection,
        pass: GarbageCollectionPass,
        context: &GarbageCollectionContext,
    ) -> Result<i64, StorageError> {
        Ok(garbage_collection_eligible(pass, context)
            .count()
            .get_result::<i64>(conn)?)
    }

    // Removes up to `limit` (unlimited if `None`) peers eligible for `pass`, returning how many were removed
    fn garbage_collection_pass(
        &self,
        conn: &mut SqliteConnection,
        pass: GarbageCollectionPass,
        context: &GarbageCollectionContext,
        mut limit: Option<i64>,
    ) -> Result<usize, StorageError> {
        let mut removed = 0usize;
        loop {
            let read_size = match limit {
                Some(limit) => min(limit, GARBAGE_COLLECTION_READ_SIZE),
                None => GARBAGE_COLLECTION_READ_SIZE,
            };
            if read_size <= 0 {
                break;
            }
            // A plain read: no write lock is held while the whole table is searched
            let candidates = garbage_collection_candidates(pass, context)
                .limit(read_size)
                .load::<i64>(conn)?;
            if candidates.is_empty() {
                break;
            }

            let mut removed_this_read = 0usize;
            for chunk in candidates.chunks(GARBAGE_COLLECTION_BATCH_SIZE) {
                let deleted = PeerDatabaseSql::delete_if_still_eligible(conn, pass, context, chunk)?;
                removed_this_read = removed_this_read.saturating_add(deleted);
            }
            if removed_this_read == 0 {
                break;
            }
            removed = removed.saturating_add(removed_this_read);
            if let Some(remaining) = limit.as_mut() {
                *remaining = remaining.saturating_sub(i64::try_from(removed_this_read).unwrap_or(i64::MAX));
            }
        }
        Ok(removed)
    }

    // Deletes those of `peer_ids` that are still eligible for `pass`, in one short write transaction. Re-checking is
    // cheap: the candidate query is restricted to these peer ids.
    fn delete_if_still_eligible(
        conn: &mut SqliteConnection,
        pass: GarbageCollectionPass,
        context: &GarbageCollectionContext,
        peer_ids: &[i64],
    ) -> Result<usize, StorageError> {
        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let still_eligible = garbage_collection_candidates(pass, context)
                .filter(peers::peer_id.eq_any(peer_ids))
                .load::<i64>(conn)?;
            PeerDatabaseSql::delete_peers_by_id(&still_eligible, conn)
        })
    }

    // Hard-deletes the given peers and their addresses, returning the number of peers deleted
    fn delete_peers_by_id(peer_ids: &[i64], conn: &mut SqliteConnection) -> Result<usize, StorageError> {
        if peer_ids.is_empty() {
            return Ok(0);
        }
        diesel::delete(multi_addresses::table.filter(multi_addresses::peer_id.eq_any(peer_ids))).execute(conn)?;
        let deleted = diesel::delete(peers::table.filter(peers::peer_id.eq_any(peer_ids))).execute(conn)?;
        Ok(deleted)
    }

    /// Reset the banned metadata for a peer, returning 'Some(node_id)' if successful, 'None' otherwise
    pub fn reset_banned(&self, node_id: &NodeId) -> Result<Option<NodeId>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.immediate_transaction::<_, StorageError, _>(|conn| {
            let affected = diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_string())))
                .set((
                    peers::banned_until.eq(None::<NaiveDateTime>),
                    peers::banned_reason.eq(String::new()),
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
                    peers::banned_reason.eq(String::new()),
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
                    .filter(multi_addresses::address_bytes.eq(address.to_vec())),
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
                    .filter(multi_addresses::address_bytes.eq(address.to_vec())),
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
                    .filter(multi_addresses::address_bytes.eq(address.to_vec())),
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
                    .filter(multi_addresses::address_bytes.eq(address.to_vec())),
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

    // Helper function to convert a Vec of left join query results into a Vec of Peer
    fn peers_from_left_join_query(
        results: Vec<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>,
    ) -> Result<Vec<Peer>, StorageError> {
        if results.is_empty() {
            return Ok(Vec::new());
        }

        let mut peer_map: HashMap<i64, (NewPeerSql, Vec<NewMultiaddrWithStatsSql>)> =
            HashMap::with_capacity(results.len());

        for (peer, addr_opt) in results {
            let entry = peer_map.entry(peer.peer_id).or_insert_with(|| (peer, Vec::new()));
            if let Some(addr) = addr_opt {
                entry.1.push(addr);
            }
        }

        Ok(PeerDatabaseSql::peers_from_rows(peer_map))
    }

    // Converts grouped peer rows into peers. A peer row that cannot be decoded is skipped with a warning rather than
    // failing every query that happens to load it.
    fn peers_from_rows(peer_map: HashMap<i64, (NewPeerSql, Vec<NewMultiaddrWithStatsSql>)>) -> Vec<Peer> {
        let mut peers = Vec::with_capacity(peer_map.len());
        for (_, (peer, addresses)) in peer_map {
            let node_id = peer.node_id.clone();
            match Peer::try_from((peer, addresses)) {
                Ok(peer) => peers.push(peer),
                Err(err) => {
                    warn!(target: LOG_TARGET, "Skipping stored peer '{node_id}' that cannot be decoded: {err}");
                },
            }
        }
        peers
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

        Ok(PeerDatabaseSql::peers_from_rows(peer_map))
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
                .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::public_key.like(format!("{partial_key}%")))
                .select((peers::all_columns, multi_addresses::all_columns.nullable()))
                .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(&mut conn)?;
        } else {
            results = peers::table
                .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::node_id.like(format!("{partial_key}%")))
                .select((peers::all_columns, multi_addresses::all_columns.nullable()))
                .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(&mut conn)?;

            let mut public_key_match = peers::table
                .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::public_key.like(format!("{partial_key}%")))
                .select((peers::all_columns, multi_addresses::all_columns.nullable()))
                .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(&mut conn)?;

            results.append(&mut public_key_match);
        }

        PeerDatabaseSql::peers_from_left_join_query(results)
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
            .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
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

        let results = query.load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(conn)?;

        PeerDatabaseSql::peers_from_left_join_query(results)
    }

    /// Return at most `n` peers from the database that are not banned and not deleted
    pub fn get_n_not_banned_or_deleted_peers(&self, number: usize) -> Result<Vec<Peer>, StorageError> {
        if number == 0 {
            return Ok(Vec::new());
        }
        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            // Step 1: at most `number` peers that have an address, limited in SQL rather than after loading every row
            let node_ids = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(
                    peers::banned_until
                        .is_null()
                        .or(peers::banned_until.lt(chrono::Utc::now().naive_utc())),
                )
                .filter(peers::deleted_at.is_null())
                .select(peers::node_id)
                .distinct()
                .limit(i64::try_from(number).unwrap_or(i64::MAX))
                .load::<String>(conn)?;

            // Step 2: load those peers with all of their addresses
            self.get_peers_by_node_ids_str(&node_ids, false, conn)
        })
    }

    /// Return available dial candidates that are communication nodes, not banned, not deleted,
    /// optionally not failed, optionally at random, and not in the excluded node IDs list. At most `max_per_netgroup`
    /// addresses are taken from any one network group (`0` disables the cap), less the number already taken from that
    /// group according to `netgroups_taken` (for a caller that tops up an earlier selection). Addresses seen since
    /// `seen_since` are taken first; an older sighting no longer counts.
    pub fn get_available_dial_candidates(
        &self,
        exclude_node_ids: &[NodeId],
        n: Option<usize>,
        transport_protocols: &[TransportProtocol],
        exclude_failed: bool,
        randomize: bool,
        max_per_netgroup: usize,
        netgroups_taken: &HashMap<String, usize>,
        seen_since: NaiveDateTime,
    ) -> Result<Vec<Peer>, StorageError> {
        if let Some(n) = n &&
            n == 0
        {
            warn!(target: LOG_TARGET, "'0' requested for 'get_available_dial_candidates'");
            return Ok(Vec::new());
        }
        let mut conn = self.connection.get_pooled_connection()?;

        // Communication nodes that are not banned and not deleted
        let mut conditions = vec![
            "(peers.banned_until IS NULL OR peers.banned_until < ?2)".to_string(),
            "peers.deleted_at IS NULL".to_string(),
            format!("peers.features & {} != 0", PeerFeatures::COMMUNICATION_NODE.to_i32()),
        ];
        if exclude_failed {
            conditions.push("multi_addresses.last_failed_reason IS NULL".to_string());
        }
        if let Some(filter_sql) = Self::build_addr_filter_sql(transport_protocols) {
            conditions.push(filter_sql);
        }
        // Exclude connected peers if provided
        if !exclude_node_ids.is_empty() {
            let excluded_hex_ids: Vec<String> = exclude_node_ids
                .iter()
                .map(|node_id| format!("'{}'", sql_escape(&node_id.to_hex())))
                .collect();
            conditions.push(format!("peers.node_id NOT IN ({})", excluded_hex_ids.join(", ")));
        }

        // Each network group (see `address_netgroup`) contributes at most `max_per_netgroup` addresses, so that one
        // operator with many addresses in one network cannot fill the candidate list. Addresses we have reached before
        // are taken first, within each group and overall.
        // A peer is admitted only if every one of its eligible addresses is within its group's budget. Admitting it
        // when any one is would let a peer with an address in a crowded group plus one in a group of its own (a
        // unique DNS name or onion address costs nothing) through, and the dialer would then dial the crowded
        // address too.
        let netgroup_cap = if max_per_netgroup == 0 {
            "1".to_string()
        } else {
            // `MAX(netgroup_rank + <already taken from this group>) <= max_per_netgroup` over the peer's rows
            let mut already_taken = "0".to_string();
            if !netgroups_taken.is_empty() {
                let mut arms = Vec::with_capacity(netgroups_taken.len());
                for (netgroup, taken) in netgroups_taken {
                    arms.push(format!("WHEN '{}' THEN {}", sql_escape(netgroup), taken));
                }
                already_taken = format!("CASE netgroup {} ELSE 0 END", arms.join(" "));
            }
            format!("MAX(netgroup_rank + {already_taken}) <= {max_per_netgroup}")
        };
        let (rank_order, final_order) = if randomize {
            ("RANDOM()", "RANDOM()")
        } else {
            ("multi_addresses.address_id", "node_id")
        };
        let limit = n.map_or(i64::MAX, |n| i64::try_from(n).unwrap_or(i64::MAX));
        let sql = format!(
            "SELECT node_id FROM ( SELECT peers.node_id AS node_id, multi_addresses.netgroup AS netgroup, \
             COALESCE(multi_addresses.last_seen >= ?1, 0) AS was_seen, ROW_NUMBER() OVER ( PARTITION BY \
             multi_addresses.netgroup ORDER BY COALESCE(multi_addresses.last_seen >= ?1, 0) DESC, {rank_order} ) AS \
             netgroup_rank FROM peers INNER JOIN multi_addresses ON multi_addresses.peer_id = peers.peer_id WHERE {} \
             ) GROUP BY node_id HAVING {netgroup_cap} ORDER BY MAX(was_seen) DESC, {final_order} LIMIT {limit}",
            conditions.join(" AND ")
        );
        // ?1: an address last seen before this no longer counts as reached; ?2: now
        let node_ids = diesel::sql_query(sql)
            .bind::<diesel::sql_types::Timestamp, _>(seen_since)
            .bind::<diesel::sql_types::Timestamp, _>(chrono::Utc::now().naive_utc())
            .load::<NodeIdRow>(&mut conn)?
            .into_iter()
            .map(|row| row.node_id)
            .collect::<Vec<_>>();

        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // A peer can have more than one valid address, so that if the join-limit-load query is not returning node IDs,
        // it will give the requested number of unique peer-adress combinations, which will result in less peer records
        // than what was required. For that reason, we need a 2nd query to fetch the peers by node IDs.
        let mut peers = self.get_peers_by_node_ids_str(&node_ids, false, &mut conn)?;
        // Keep the order chosen above (previously reached first, then random)
        let mut position = HashMap::with_capacity(node_ids.len());
        for (index, node_id) in node_ids.iter().enumerate() {
            position.insert(node_id.clone(), index);
        }
        peers.sort_by_key(|peer| position.get(&peer.node_id.to_hex()).copied().unwrap_or(usize::MAX));
        Ok(peers)
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
            .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::node_id.eq(node_id))
            .select((peers::all_columns, multi_addresses::all_columns.nullable()))
            .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(conn)?;

        Ok(PeerDatabaseSql::peers_from_left_join_query(results)?.first().cloned())
    }

    /// Get all peers based on a list of their node_ids
    pub fn get_peers_by_node_ids(&self, node_ids: &[NodeId]) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let node_ids_hex = node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
        self.get_peers_by_node_ids_str(&node_ids_hex, false, &mut conn)
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
        external_addresses_only: bool,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
    ) -> Result<Vec<Peer>, StorageError> {
        if external_addresses_only {
            let results = peers::table
                .left_outer_join(
                    multi_addresses::table.on(multi_addresses::peer_id
                        .eq(peers::peer_id)
                        .and(multi_addresses::is_external.eq(true))),
                )
                .filter(peers::node_id.eq_any(node_ids))
                .select((peers::all_columns, multi_addresses::all_columns.nullable()))
                .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(conn)?;
            PeerDatabaseSql::peers_from_left_join_query(results)
        } else {
            let results = peers::table
                .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::node_id.eq_any(node_ids))
                .select((peers::all_columns, multi_addresses::all_columns.nullable()))
                .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(conn)?;
            PeerDatabaseSql::peers_from_left_join_query(results)
        }
    }

    /// Get all banned peers
    pub fn get_banned_peers(&self) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(
                peers::banned_until
                    .is_not_null()
                    .and(peers::banned_until.gt(chrono::Utc::now().naive_utc())),
            )
            .select((peers::all_columns, multi_addresses::all_columns.nullable()))
            .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(&mut conn)?;

        PeerDatabaseSql::peers_from_left_join_query(results)
    }

    /// Get a peer by its public key
    pub fn get_peer_by_public_key(&self, public_key: &CommsPublicKey) -> Result<Option<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let public_key = public_key.to_hex();
        let results = peers::table
            .left_outer_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(peers::public_key.eq(public_key))
            .select((peers::all_columns, multi_addresses::all_columns.nullable()))
            .load::<(NewPeerSql, Option<NewMultiaddrWithStatsSql>)>(&mut conn)?;
        if results.is_empty() {
            return Ok(None);
        }

        // Group addresses for the peer
        let peer_query = results.first().expect("already checked").0.clone();
        let addresses_query = results
            .iter()
            .filter_map(|(_, address)| address.clone())
            .collect::<Vec<_>>();

        Ok(Some(Peer::try_from((peer_query, addresses_query))?))
    }

    /// Get all addresses for a peer based on its node_id
    pub fn get_addresses(&self, node_id: &NodeId) -> Result<MultiaddressesWithStats, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_id_hex = node_id.to_hex();
            let peer_id = peers::table
                .filter(peers::node_id.eq(&node_id_hex))
                .select(peers::peer_id)
                .first::<i64>(conn)?;
            let addresses_query: Vec<NewMultiaddrWithStatsSql> = multi_addresses::table
                .filter(multi_addresses::peer_id.eq(peer_id))
                .load::<NewMultiaddrWithStatsSql>(conn)?;

            Ok(multiaddresses_from_rows(&node_id_hex, addresses_query))
        })
    }

    // Get the closest `n` active peer ids (have been seen, optionally within a threshold, not banned, not deleted,
    // optional features, optionally that have at least one external address).
    fn get_active_peer_node_ids(
        &self,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        peer_flags: Option<PeerFlags>,
        stale_peer_threshold: Option<Duration>,
        exclude_if_all_address_failed: bool,
        at_least_one_external_addresses: bool,
        n: Option<usize>,
        conn: &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        transport_protocols: &[TransportProtocol],
    ) -> Result<Vec<String>, StorageError> {
        if let Some(n) = n &&
            n == 0
        {
            warn!(target: LOG_TARGET, "'0' requested for 'get_active_peer_node_ids'");
            return Ok(Vec::new());
        }
        let excluded_node_ids_hex = excluded_peers.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
        let addr_filter_sql = Self::build_addr_filter_sql(transport_protocols);

        // Step 1: Retrieve relevant node_ids
        let mut query = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(
                peers::banned_until
                    .is_null()
                    .or(peers::banned_until.lt(chrono::Utc::now().naive_utc())),
            )
            .filter(peers::deleted_at.is_null())
            .filter(peers::node_id.ne_all(excluded_node_ids_hex))
            .distinct()
            .into_boxed(); // Enables dynamic query building

        if let Some(flags) = peer_flags {
            query = query.filter(peers::flags.eq(flags.to_i32()));
        }

        if let Some(filter_sql) = addr_filter_sql {
            query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&filter_sql));
        }

        if exclude_if_all_address_failed {
            query = query
                .filter(multi_addresses::last_seen.is_not_null())
                .filter(multi_addresses::last_failed_reason.is_null());
        }

        if at_least_one_external_addresses {
            query = query.filter(multi_addresses::is_external.eq(true));
        }

        if let Some(threshold) = stale_peer_threshold {
            let threshold = min(
                threshold,
                Duration::from_secs(i64::MAX.unsigned_abs().saturating_sub(1)),
            );
            let stale_threshold = chrono::Utc::now()
                .naive_utc()
                .checked_sub_signed(chrono::Duration::from_std(threshold).unwrap_or(TimeDelta::MAX))
                .unwrap_or(chrono::NaiveDateTime::MIN);
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
            // Weighted-random ordering that biases towards more recently-seen peers while still keeping the
            // selection random (for network diversity / anti-eclipse). For each candidate we compute
            //   score = age_in_days * U,  U ~ Uniform(0, 1)
            // and pick the `n` smallest scores. A fresher peer (small age) gets a small score regardless of its
            // random draw, so it is more likely to be served; a stale peer needs a small draw to compete. NULL
            // `last_seen` is treated as maximally old so never-seen rows sort last. `RANDOM()` returns a signed
            // 64-bit integer, so `RANDOM() / 2^64` is in (-0.5, 0.5) and `+ 0.5` shifts it to (0, 1).
            // The age is clamped to >= 0 with `max(0.0, ...)`: a future-dated `last_seen` (clock skew or a peer
            // intentionally claiming a future timestamp) would otherwise produce a negative age and hence a negative
            // score that always sorts first, letting such peers bypass the weighted-random selection. Clamping treats
            // any future date as "brand new" instead.
            query = query
                .order_by(diesel::dsl::sql::<diesel::sql_types::Double>(
                    "max(0.0, julianday('now') - julianday(COALESCE(multi_addresses.last_seen, '1970-01-01 \
                     00:00:00'))) * (0.5 + RANDOM() / 18446744073709551616.0)",
                ))
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

    /// Get `n` active random peers, ordered by their distance to the given node ID.
    pub fn get_n_random_active_peers(
        &self,
        n: usize,
        excluded_peers: &[NodeId],
        features: Option<PeerFeatures>,
        peer_flags: Option<PeerFlags>,
        stale_peer_threshold: Option<Duration>,
        external_addresses_only: bool,
        transport_protocols: &[TransportProtocol],
    ) -> Result<Vec<Peer>, StorageError> {
        if n == 0 {
            warn!(target: LOG_TARGET, "'0' requested for 'get_n_random_active_peers'");
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;

        conn.transaction::<_, StorageError, _>(|conn| {
            let node_ids_hex = self.get_active_peer_node_ids(
                excluded_peers,
                features,
                peer_flags,
                stale_peer_threshold,
                true,
                external_addresses_only,
                Some(n),
                conn,
                transport_protocols,
            )?;

            let peers = self.get_peers_by_node_ids_str(&node_ids_hex, external_addresses_only, conn)?;

            Ok(peers)
        })
    }

    /// Get a random set of `n` peers from the database that are not banned and not deleted and have at least one
    /// external address
    pub fn get_n_random_peers(
        &self,
        n: usize,
        exclude_node_ids: &[NodeId],
        peer_flags: Option<PeerFlags>,
        transport_protocols: &[TransportProtocol],
        known_good: bool,
    ) -> Result<Vec<Peer>, StorageError> {
        if n == 0 {
            warn!(target: LOG_TARGET, "'0' requested for 'get_n_random_peers'");
            return Ok(Vec::new());
        }

        let mut conn = self.connection.get_pooled_connection()?;
        let exclude_node_ids = exclude_node_ids.iter().map(|id| id.to_hex()).collect::<Vec<_>>();
        let addr_filter_sql = Self::build_addr_filter_sql(transport_protocols);
        conn.transaction::<_, StorageError, _>(|conn| {
            // Step 1: Filtered, random and truncated list of node_ids
            let mut query = peers::table
                .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
                .filter(peers::deleted_at.is_null())
                .filter(
                    peers::banned_until
                        .is_null()
                        .or(peers::banned_until.lt(chrono::Utc::now().naive_utc())),
                )
                .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                    "features & {} != 0",
                    PeerFeatures::COMMUNICATION_NODE.to_i32()
                )))
                .filter(peers::node_id.ne_all(exclude_node_ids))
                .order_by(diesel::dsl::sql::<diesel::sql_types::Integer>("RANDOM()"))
                .limit(i64::try_from(n).unwrap_or(i64::MAX))
                .select(peers::node_id)
                .distinct()
                .into_boxed();

            if known_good {
                // "Known good" has to mean recently reachable, not merely reachable once: a peer
                // last seen hours ago has usually gone, and dialing it is not free — on a Tor-heavy
                // network it costs a slow rendezvous that fails, crowding out a live peer. Callers
                // that cannot fill `n` from this set fall back to a relaxed query, so narrowing it
                // costs nothing when few recent peers are known.
                let cutoff = chrono::Utc::now()
                    .naive_utc()
                    .checked_sub_signed(KNOWN_GOOD_LAST_SEEN_WINDOW)
                    .unwrap_or(chrono::NaiveDateTime::MIN);
                query = query.filter(multi_addresses::last_seen.gt(cutoff));
            }
            if let Some(flags) = peer_flags {
                query = query.filter(peers::flags.eq(flags.to_i32()));
            }

            if let Some(filter_sql) = addr_filter_sql {
                query = query.filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&filter_sql));
            }

            let node_ids: Vec<String> = query.load::<String>(conn)?;

            if node_ids.is_empty() {
                return Ok(Vec::new());
            }

            // Step 2: Load full peer + addresses only for selected node_ids
            self.get_peers_by_node_ids_str(&node_ids, true, conn)
        })
    }

    /// Get all the seed peers
    pub fn get_seed_peers(&self) -> Result<Vec<Peer>, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;

        // Perform a join query to fetch peers and their addresses
        let results = peers::table
            .inner_join(multi_addresses::table.on(multi_addresses::peer_id.eq(peers::peer_id)))
            .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(&format!(
                "flags & {} != 0",
                PeerFlags::SEED.to_i32()
            )))
            .load::<(NewPeerSql, NewMultiaddrWithStatsSql)>(&mut conn)?;

        PeerDatabaseSql::peers_from_join_query(results)
    }

    fn count_peers(&self) -> Result<i64, StorageError> {
        let mut conn = self.connection.get_pooled_connection()?;
        Ok(peers::table.count().get_result::<i64>(&mut conn)?)
    }

    /// Get the size of the peer database
    pub fn size(&self) -> usize {
        usize::try_from(self.count_peers().unwrap_or_default()).unwrap_or_default()
    }

    /// Builds an OR-ed SQL clause like:
    /// "multi_addresses.address LIKE '/ip4%' OR multi_addresses.address LIKE '/ip6%'"
    fn build_addr_filter_sql(transport_protocols: &[TransportProtocol]) -> Option<String> {
        if transport_protocols.is_empty() {
            return None;
        }

        let conditions: Vec<String> = transport_protocols
            .iter()
            .map(|protocol| format!("multi_addresses.address LIKE '{}%'", sql_escape(protocol.get_prefix())))
            .collect();

        Some(format!("({})", conditions.join(" OR ")))
    }
}

/// True if the address's text form reads back as exactly this address.
///
/// The `address` text column is what older versions read, and one row they cannot parse makes their peer queries fail,
/// so an address without a lossless text form is not stored at all (with a warning). Claim validation already rejects
/// such addresses, so this is only a backstop.
fn has_lossless_text(address: &Multiaddr) -> bool {
    let text = address.to_string();
    let reads_back = Multiaddr::from_str(&text).is_ok_and(|parsed| parsed.to_vec() == address.to_vec());
    if !reads_back {
        warn!(
            target: LOG_TARGET,
            "Not storing address {text:?}: its text form does not read back as the same address"
        );
    }
    reads_back
}

/// The longest ban reason stored, in bytes. Reasons often quote what a peer sent.
const MAX_BAN_REASON_BYTES: usize = 256;

/// A ban reason as stored: no control, format or line-breaking characters, and at most [`MAX_BAN_REASON_BYTES`].
fn sanitize_ban_reason(reason: &str) -> String {
    sanitize_for_display(reason, MAX_BAN_REASON_BYTES)
}

fn sql_escape(input: &str) -> String {
    input.replace('\'', "''")
}

#[derive(QueryableByName)]
struct NodeIdRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    node_id: String,
}

#[derive(Clone, Debug, Selectable, Queryable, Insertable, PartialEq, Eq)]
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

#[derive(Clone, Debug, Queryable, Insertable, PartialEq, Eq)]
#[diesel(table_name = peers)]
pub struct NewPeerSql {
    pub peer_id: i64,
    pub public_key: String,
    pub node_id: String,
    pub flags: i32,
    pub banned_until: Option<NaiveDateTime>,
    pub banned_reason: String,
    pub features: i32,
    pub supported_protocols: String,
    pub added_at: NaiveDateTime,
    pub user_agent: String,
    pub metadata: Option<Vec<u8>>,
    pub deleted_at: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = peers)]
pub struct UpdatePeerSql {
    pub node_id: Option<String>,
    pub flags: Option<i32>,
    pub banned_until: Option<Option<NaiveDateTime>>,
    pub banned_reason: Option<String>,
    pub features: Option<i32>,
    pub supported_protocols: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Option<Option<Vec<u8>>>,
    pub deleted_at: Option<Option<NaiveDateTime>>,
}

#[derive(Clone, Debug)]
pub struct UpdatePeerWithAddressesSql {
    pub peer: UpdatePeerSql,
    pub addresses: Vec<UpdateMultiaddrWithStatsSql>,
}

#[derive(Clone, Debug, Queryable, Insertable, PartialEq, Eq)]
#[diesel(table_name = multi_addresses)]
pub struct NewMultiaddrWithStatsSql {
    pub address_id: Option<i32>,
    pub peer_id: i64,
    pub address: String,
    pub is_external: bool,
    pub last_seen: Option<NaiveDateTime>,
    pub connection_attempts: i32,
    pub avg_initial_dial_time: Option<i64>,
    pub initial_dial_time_sample_count: i32,
    pub avg_latency: Option<i64>,
    pub latency_sample_count: i32,
    pub last_attempted: Option<NaiveDateTime>,
    pub last_failed_reason: Option<String>,
    pub quality_score: Option<i32>,
    pub source: String,
    pub address_bytes: Vec<u8>,
    pub netgroup: String,
}

#[derive(Clone, Debug, AsChangeset, PartialEq, Eq)]
#[diesel(table_name = multi_addresses)]
pub struct UpdateMultiaddrWithStatsSql {
    pub address: Option<String>,
    pub is_external: Option<bool>,
    pub last_seen: Option<Option<NaiveDateTime>>,
    pub connection_attempts: Option<i32>,
    pub avg_initial_dial_time: Option<Option<i64>>,
    pub initial_dial_time_sample_count: Option<i32>,
    pub avg_latency: Option<Option<i64>>,
    pub latency_sample_count: Option<i32>,
    pub last_attempted: Option<Option<NaiveDateTime>>,
    pub last_failed_reason: Option<Option<String>>,
    pub quality_score: Option<Option<i32>>,
    pub source: Option<String>,
    pub address_bytes: Option<Vec<u8>>,
    pub netgroup: Option<String>,
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
                warn!(target: LOG_TARGET, "duration_to_i64_ms_infallible {duration:?} conversion error: {e}");
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
            address: Some(address.to_string()),
            is_external: Some(address.is_external()),
            last_seen: Some(address.last_seen()),
            connection_attempts: Some(u32_to_i32_infallible(address.connection_attempts())),
            avg_initial_dial_time: Some(duration_to_i64_ms_infallible(address.avg_initial_dial_time())),
            initial_dial_time_sample_count: Some(u32_to_i32_infallible(address.initial_dial_time_sample_count())),
            avg_latency: Some(duration_to_i64_ms_infallible(address.avg_latency())),
            latency_sample_count: Some(u32_to_i32_infallible(address.latency_sample_count())),
            last_attempted: Some(address.last_attempted()),
            last_failed_reason: Some(address.last_failed_reason().map(|v| v.to_string())),
            quality_score: Some(address.quality_score()),
            source: Some(serde_json::to_string(&address.source()).unwrap_or_default()),
            address_bytes: Some(address.address().to_vec()),
            netgroup: Some(address_netgroup(address.address())),
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
            multiaddresses_from_rows(&peer_query.node_id, addresses_query),
            PeerFlags::from_bits(u8::try_from(peer_query.flags)?)
                .ok_or_else(|| StorageError::UnexpectedResult("Peer flags are invalid".to_string()))?,
            peer_query.banned_until,
            peer_query.banned_reason,
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

/// Converts a peer's stored address rows into its address list.
///
/// A row that cannot be decoded is skipped with a warning instead of failing the whole read: one bad row used to make
/// every query that loaded it fail. A peer left with no usable address gets the empty placeholder address, as a peer
/// stored without addresses always has, so it is never silently dropped from a result.
fn multiaddresses_from_rows(node_id: &str, addresses_query: Vec<NewMultiaddrWithStatsSql>) -> MultiaddressesWithStats {
    let mut addresses = Vec::with_capacity(addresses_query.len());
    for addr in addresses_query {
        let address_text = addr.address.clone();
        match multiaddr_with_stats_from_row(addr) {
            Ok(address) => addresses.push(address),
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Skipping stored address {address_text:?} of peer '{node_id}' that cannot be decoded: {err}"
                );
                #[cfg(feature = "metrics")]
                metrics::undecodable_address_rows().inc();
            },
        }
    }
    if addresses.is_empty() {
        addresses.push(MultiaddrWithStats::new(Multiaddr::empty(), PeerAddressSource::Config));
    }
    MultiaddressesWithStats::from(addresses)
}

fn multiaddr_with_stats_from_row(addr: NewMultiaddrWithStatsSql) -> Result<MultiaddrWithStats, StorageError> {
    let address = Multiaddr::try_from(addr.address_bytes).map_err(|e| StorageError::UnexpectedResult(e.to_string()))?;
    if address.is_empty() {
        return Err(StorageError::UnexpectedResult("Address bytes are empty".to_string()));
    }
    Ok(MultiaddrWithStats::new_with_stats(
        address,
        addr.last_seen,
        u32::try_from(addr.connection_attempts)?,
        i64_to_duration(addr.avg_initial_dial_time)?,
        u32::try_from(addr.initial_dial_time_sample_count)?,
        i64_to_duration(addr.avg_latency)?,
        u32::try_from(addr.latency_sample_count)?,
        addr.last_attempted,
        addr.last_failed_reason,
        addr.quality_score,
        serde_json::from_str(&addr.source).map_err(StorageError::JsonError)?,
    ))
}

impl From<(UpdateMultiaddrWithStatsSql, i64)> for NewMultiaddrWithStatsSql {
    fn from((address, peer_id): (UpdateMultiaddrWithStatsSql, i64)) -> Self {
        NewMultiaddrWithStatsSql {
            address_id: None,
            peer_id,
            address: address.address.unwrap_or_default(),
            is_external: address.is_external.unwrap_or_default(),
            last_seen: address.last_seen.unwrap_or_default(),
            connection_attempts: address.connection_attempts.unwrap_or_default(),
            avg_initial_dial_time: address.avg_initial_dial_time.unwrap_or_default(),
            initial_dial_time_sample_count: address.initial_dial_time_sample_count.unwrap_or_default(),
            avg_latency: address.avg_latency.unwrap_or_default(),
            latency_sample_count: address.latency_sample_count.unwrap_or_default(),
            last_attempted: address.last_attempted.unwrap_or_default(),
            last_failed_reason: address.last_failed_reason.unwrap_or_default(),
            quality_score: address.quality_score.unwrap_or_default(),
            source: address.source.unwrap_or_default(),
            address_bytes: address.address_bytes.unwrap_or_default(),
            netgroup: address.netgroup.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use std::{collections::HashMap, time::Duration};

    use chrono::TimeDelta;
    use diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl};
    use tari_common_sqlite::connection::DbConnection;
    use tari_utilities::{ByteArray, hex::Hex};

    use crate::{
        multiaddr::Multiaddr,
        net_address::{MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{
            NodeId,
            Peer,
            PeerFeatures,
            PeerFlags,
            create_test_peer,
            create_test_peer_add_internal_addresses,
            create_test_peer_internal_addresses_only,
            database::{MIGRATIONS, NewMultiaddrWithStatsSql, NewPeerSql, PeerDatabaseSql},
            manager::{create_peer_address_source_with_claim, create_test_peer_with_onion_address},
            storage::{
                database::{
                    ADDRESS_BYTES_VERSION_KEY,
                    ADDRESS_CLASSIFIER_VERSION_KEY,
                    GarbageCollectionContext,
                    GarbageCollectionPass,
                    GarbageCollectionStats,
                    duration_to_i64_ms_infallible,
                    garbage_collection_candidates,
                    u32_to_i32_infallible,
                },
                schema::{db_metadata, multi_addresses, peers},
            },
        },
        protocol::ProtocolId,
        types::{CommsPublicKey, TransportProtocol},
    };

    #[test]
    fn test_stored_address_externality_is_reclassified_once_per_classifier_version() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peers_db = PeerDatabaseSql::new(db_connection.clone(), &this_peer).unwrap();

        let external: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let internal: Multiaddr = "/ip4/192.168.1.2/tcp/18189".parse().unwrap();
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peer.addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec![external.clone(), internal.clone()],
            &PeerAddressSource::Config,
        );
        peers_db.add_or_update_peer(peer).unwrap();

        // Rows written by a version whose classifier disagreed with the current one. They are never
        // handed back to the update path, so only an explicit pass can fix them.
        let misclassify = |conn: &mut _| {
            for (address, is_external) in [(&internal, true), (&external, false)] {
                diesel::update(multi_addresses::table.filter(multi_addresses::address.eq(address.to_string())))
                    .set(multi_addresses::is_external.eq(is_external))
                    .execute(conn)
                    .unwrap();
            }
        };
        let is_external = |conn: &mut _, address: &Multiaddr| {
            multi_addresses::table
                .filter(multi_addresses::address.eq(address.to_string()))
                .select(multi_addresses::is_external)
                .first::<bool>(conn)
                .unwrap()
        };

        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        misclassify(&mut conn);
        // An upgrade from a build that predates the classifier version marker.
        diesel::delete(db_metadata::table.filter(db_metadata::key.eq(ADDRESS_CLASSIFIER_VERSION_KEY)))
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        // Opening the database reclassifies them without waiting for a newer signed claim.
        let peers_db = PeerDatabaseSql::new(db_connection.clone(), &this_peer).unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        assert!(is_external(&mut conn, &external));
        assert!(!is_external(&mut conn, &internal));

        // The applied version is recorded, so a later start does no work at all.
        misclassify(&mut conn);
        drop(conn);
        let peers_db = PeerDatabaseSql::new(db_connection, &this_peer).unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        assert!(!is_external(&mut conn, &external));
        assert!(is_external(&mut conn, &internal));
    }

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
        let new_addr: Multiaddr = "/ip4/127.0.0.1/udt/sctp/5678".parse().unwrap();
        new_peer
            .addresses
            .add_or_update_addresses(std::slice::from_ref(&new_addr), &PeerAddressSource::Config);
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

    #[test]
    fn test_filtering_peers_by_transport_protocols() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        let transport_protocols = vec![TransportProtocol::Ipv4, TransportProtocol::Ipv6];

        // Create new node and wallet peers with internal and external addresses
        for _i in 0..20 {
            let peer = create_test_peer_add_internal_addresses(false, PeerFeatures::COMMUNICATION_NODE);
            peers_db.add_or_update_peer(peer).unwrap();
            let peer = create_test_peer_add_internal_addresses(false, PeerFeatures::COMMUNICATION_CLIENT);
            peers_db.add_or_update_peer(peer).unwrap();
        }
        assert_eq!(peers_db.size(), 40);

        // Assert that retrieved peers have internal and external addresses
        let nodes_with_all_addresses = peers_db
            .get_n_random_active_peers(100, &[], None, None, None, false, &transport_protocols)
            .unwrap();
        assert_eq!(nodes_with_all_addresses.len(), 40);

        let nodes_with_onion_addresses = peers_db
            .get_n_random_active_peers(100, &[], None, None, None, false, &[TransportProtocol::Onion])
            .unwrap();
        assert!(nodes_with_onion_addresses.is_empty());

        // - Has external address
        assert!(
            nodes_with_all_addresses
                .iter()
                .all(|p| { p.addresses.addresses().iter().any(|addr| addr.is_external()) })
        );
        // - Has internal address
        assert!(
            nodes_with_all_addresses
                .iter()
                .all(|p| { p.addresses.addresses().iter().any(|addr| !addr.is_external()) })
        );

        // Add peer with onion addresses only
        let onion_peer = create_test_peer_with_onion_address(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(onion_peer).unwrap();
        let node_with_onion_addresses = peers_db
            .get_n_random_active_peers(100, &[], None, None, None, false, &[TransportProtocol::Onion])
            .unwrap();
        assert_eq!(node_with_onion_addresses.len(), 1);
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
        //         Duration::from_secs(rand::rng().random_range(1000..9000)),
        //         "Misbehave".to_string(),
        //     );
        //     peer.supported_protocols
        //         .push(ProtocolId::from_static(b"Test Protocol 1.0"));
        //     peer.metadata
        //         .insert(1, vec![1, 2, rand::rng().random_range(1..100)]);
        //     peer.metadata
        //         .insert(2, vec![4, 5, rand::rng().random_range(1..100)]);
        //     // - add another multi-address
        //     let n = [
        //         rand::rng().random_range(1..9),
        //         rand::rng().random_range(1..9),
        //         rand::rng().random_range(1..9),
        //         rand::rng().random_range(1..9),
        //     ];
        //     let new_addr_str = format!("/ip4/{}.{}.{}.{}/udt/sctp/{0}{1}{2}{3}", n[0], n[1], n[2], n[3]);
        //     peer.addresses
        //         .add_address(&new_addr_str.parse().unwrap(), &PeerAddressSource::Config);
        //     // - new stats for the first multi-address
        //     let mut address_to_update = peer.addresses.addresses().first().unwrap().clone();
        //     address_to_update.update_latency(Duration::from_millis(rand::rng().random_range(100..1000)));
        //     address_to_update.update_initial_dial_time(Duration::from_millis(rand::rng().random_range(100..
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
    fn test_seed_peer_exclusion() {
        let transport_protocols = TransportProtocol::get_all();
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
            if i % 3 == 0 {
                peer.flags = PeerFlags::SEED;
            }
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }

        // All peers as random
        let random_peers = peers_db
            .get_n_random_peers(12, &[], None, &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers.len(), 12);

        // All seed peers only as random
        let random_peers = peers_db
            .get_n_random_peers(12, &[], Some(PeerFlags::SEED), &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers.len(), 4);
        assert!(random_peers.iter().all(|p| p.flags == PeerFlags::SEED));

        // One seed peer as random
        let random_peers = peers_db
            .get_n_random_peers(1, &[], Some(PeerFlags::SEED), &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers[0].flags, PeerFlags::SEED);

        // All normal peers only as random
        let random_peers = peers_db
            .get_n_random_peers(12, &[], Some(PeerFlags::NONE), &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers.len(), 8);
        assert!(random_peers.iter().all(|p| p.flags == PeerFlags::NONE));

        // One normal peer as random
        let random_peers = peers_db
            .get_n_random_peers(1, &[], Some(PeerFlags::NONE), &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers[0].flags, PeerFlags::NONE);
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
        let transport_protocols = TransportProtocol::get_all();

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
        peers_db.soft_delete_peer(&node_peers[1].node_id).unwrap();
        let deleted_peer = peers_db.get_peer_by_node_id(&node_peers[1].node_id).unwrap().unwrap();
        assert!(deleted_peer.deleted_at.is_some());

        peers_db.soft_delete_peer(&wallet_peers[1].node_id).unwrap();
        let deleted_peer = peers_db.get_peer_by_node_id(&wallet_peers[1].node_id).unwrap().unwrap();
        assert!(deleted_peer.deleted_at.is_some());

        // Test 'peer_exists_by_node_id'
        assert!(
            peers_db
                .peer_exists_by_node_id(&node_peers[2].node_id)
                .unwrap()
                .is_some()
        );

        // Test 'get_peer_by_public_key'
        let peer = peers_db
            .get_peer_by_public_key(&node_peers[3].public_key)
            .unwrap()
            .unwrap();
        assert_eq!(peer, node_peers[3]);

        // Test 'peer_exists_by_public_key'
        assert!(
            peers_db
                .peer_exists_by_public_key(&node_peers[4].public_key)
                .unwrap()
                .is_some()
        );

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
        assert!(
            !n_peers
                .iter()
                .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id)
        );
        assert!(
            !n_peers
                .iter()
                .any(|n| n.node_id == wallet_peers[1].node_id || n.node_id == wallet_peers[4].node_id)
        );

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

        // Test 'get_seed_peers'
        let seed_peers = peers_db.get_seed_peers().unwrap();
        assert_eq!(seed_peers.len(), 3);
        for peer in &seed_peers {
            assert!(peer.is_seed());
        }

        // Test 'random_peers_sqlite'
        let random_peers = peers_db
            .get_n_random_peers(5, &[node_peers[0].node_id.clone()], None, &transport_protocols, false)
            .unwrap();
        assert_eq!(random_peers.len(), 5);
        // Verify deleted & banned
        assert!(
            !random_peers
                .iter()
                .any(|n| n.node_id == node_peers[1].node_id || n.node_id == node_peers[4].node_id)
        );
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
            assert!(peer.last_connect_attempt().is_none(), "peer: {peer}");
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

    #[test]
    fn a_shorter_ban_does_not_shorten_a_longer_one() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(peer.clone()).unwrap();
        let banned_until = |peers_db: &PeerDatabaseSql| {
            peers_db
                .get_peer_by_node_id(&peer.node_id)
                .unwrap()
                .unwrap()
                .banned_until
        };

        // A long ban, then a short one: the long one stays, with its reason
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(24 * 60 * 60), "long".to_string())
            .unwrap()
            .unwrap();
        let long_ban = banned_until(&peers_db).unwrap();
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(60), "short".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(banned_until(&peers_db), Some(long_ban));
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert_eq!(stored.banned_reason, "long");

        // A longer ban does extend it
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(48 * 60 * 60), "longer".to_string())
            .unwrap()
            .unwrap();
        assert!(banned_until(&peers_db).unwrap() > long_ban);

        // An indefinite ban is never shortened either
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(u64::MAX), "forever".to_string())
            .unwrap()
            .unwrap();
        let indefinite = banned_until(&peers_db).unwrap();
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(60), "short".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(banned_until(&peers_db), Some(indefinite));

        // Unbanning still clears it, and a new short ban then applies
        peers_db.reset_banned(&peer.node_id).unwrap().unwrap();
        assert_eq!(banned_until(&peers_db), None);
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(60), "short".to_string())
            .unwrap()
            .unwrap();
        assert!(banned_until(&peers_db).unwrap() < long_ban);

        // A peer that is not stored is reported as such
        let unknown = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        assert!(
            peers_db
                .set_banned(&unknown.node_id, Duration::from_secs(60), "short".to_string())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_handshake_ban_does_not_shorten_a_longer_one() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(peer.clone()).unwrap();
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(24 * 60 * 60), "long".to_string())
            .unwrap()
            .unwrap();
        let long_ban = peers_db
            .get_peer_by_node_id(&peer.node_id)
            .unwrap()
            .unwrap()
            .banned_until
            .unwrap();

        // A shorter handshake ban keeps the long one, with its reason
        peers_db
            .ban_or_add_banned_peer(&peer.public_key, Duration::from_secs(60), "short".to_string())
            .unwrap();
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert_eq!(stored.banned_until, Some(long_ban));
        assert_eq!(stored.banned_reason, "long");

        // A longer one extends it
        peers_db
            .ban_or_add_banned_peer(
                &peer.public_key,
                Duration::from_secs(48 * 60 * 60),
                "longer".to_string(),
            )
            .unwrap();
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert!(stored.banned_until.unwrap() > long_ban);
        assert_eq!(stored.banned_reason, "longer");
    }

    #[test]
    fn test_get_duplicate_addresses_allowed() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node peers
        let mut node_peers = Vec::with_capacity(5);
        for _i in 0..5 {
            let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }

        // Create a new peer with duplicate addresses
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let duplicate_address = node_peers[0]
            .addresses
            .addresses()
            .iter()
            .map(|v| v.address())
            .cloned()
            .collect::<Vec<_>>();
        for address in &duplicate_address {
            peer.addresses
                .add_or_update_addresses(std::slice::from_ref(address), &PeerAddressSource::Config)
        }
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        // Verify the new peer has the duplicate address
        let peer_from_db = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        let peer_addresses = peer_from_db
            .addresses
            .addresses()
            .iter()
            .map(|v| v.address())
            .cloned()
            .collect::<Vec<_>>();
        assert!(duplicate_address.iter().any(|v| peer_addresses.contains(v)));

        // Verify original peer still has the address
        let original_peer_from_db = peers_db.get_peer_by_node_id(&node_peers[0].node_id).unwrap().unwrap();
        let original_peer_addresses = original_peer_from_db
            .addresses
            .addresses()
            .iter()
            .map(|v| v.address())
            .cloned()
            .collect::<Vec<_>>();
        assert!(duplicate_address.iter().any(|v| original_peer_addresses.contains(v)));
    }

    #[test]
    fn discovery_syncing_peers_with_external_addresses_only() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create new node and wallet peers with internal and external addresses
        for _i in 0..20 {
            let peer = create_test_peer_add_internal_addresses(false, PeerFeatures::COMMUNICATION_NODE);
            peers_db.add_or_update_peer(peer).unwrap();
            let peer = create_test_peer_add_internal_addresses(false, PeerFeatures::COMMUNICATION_CLIENT);
            peers_db.add_or_update_peer(peer).unwrap();
        }
        assert_eq!(peers_db.size(), 40);

        // Assert that retrieved peers have internal and external addresses
        let nodes_with_all_addresses = peers_db
            .get_n_random_active_peers(100, &[], None, None, None, false, &[])
            .unwrap();
        assert_eq!(nodes_with_all_addresses.len(), 40);
        // - Has external address
        assert!(
            nodes_with_all_addresses
                .iter()
                .all(|p| { p.addresses.addresses().iter().any(|addr| addr.is_external()) })
        );
        // - Has internal address
        assert!(
            nodes_with_all_addresses
                .iter()
                .all(|p| { p.addresses.addresses().iter().any(|addr| !addr.is_external()) })
        );

        // Add peers with internal addresses only
        for _i in 0..4 {
            let peer = create_test_peer_internal_addresses_only(false, PeerFeatures::COMMUNICATION_NODE);
            peers_db.add_or_update_peer(peer).unwrap();
            let peer = create_test_peer_internal_addresses_only(false, PeerFeatures::COMMUNICATION_CLIENT);
            peers_db.add_or_update_peer(peer).unwrap();
        }
        // Add peers with no addresses
        for _i in 0..4 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            peer.addresses = MultiaddressesWithStats::new(vec![]);
            peers_db.add_or_update_peer(peer).unwrap();
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_CLIENT);
            peer.addresses = MultiaddressesWithStats::new(vec![]);
            peers_db.add_or_update_peer(peer).unwrap();
        }
        assert_eq!(peers_db.size(), 56);
        let all_peers = peers_db.get_all_peers(None).unwrap();
        assert_eq!(all_peers.len(), 56);

        // Assert that retrieved peers have external addresses only
        let nodes_with_external_addresses_only = peers_db
            .get_n_random_active_peers(100, &[], None, None, None, true, &[])
            .unwrap();
        assert_eq!(nodes_with_external_addresses_only.len(), 40);
        // - Has external address only
        assert!(
            nodes_with_external_addresses_only
                .iter()
                .all(|p| { p.addresses.addresses().iter().all(|addr| addr.is_external()) })
        );
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

    #[test]
    fn it_correctly_updates_none_as_null() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create a new peer and add a failure reason
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let address = peer.last_address_used().unwrap();
        peer.addresses
            .mark_failed_connection_attempt(&address, "This has failed".to_string());

        // Add the peer to the db
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        // Ensure the failure reason is still present when read from the db
        let peer_from_db = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        let last_failed_reason = peer_from_db
            .addresses
            .addresses()
            .iter()
            .find(|a| a.address() == &address)
            .and_then(|a| a.last_failed_reason());
        assert!(last_failed_reason.is_some());

        // Now mark the address as seen, which should clear the failure reason, and update the db
        peer.addresses.mark_last_seen_now(&address);
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        // Read the peer from the db and ensure the failure reason is cleared
        let peer_from_db = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        let last_failed_reason = peer_from_db
            .addresses
            .addresses()
            .iter()
            .find(|a| a.address() == &address)
            .and_then(|a| a.last_failed_reason());
        assert!(last_failed_reason.is_none());
    }

    #[test]
    fn it_correctly_handles_a_peer_without_any_address() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();

        // Create a new peer and remove all addresses
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peer.addresses = MultiaddressesWithStats::new(vec![]);

        // Add the peer to the db
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        // Retrieve the peer from the db
        let peer_from_db = peers_db.get_peer_by_node_id(&peer.node_id).unwrap();
        assert!(peer_from_db.is_some());
        let peer_from_db = peer_from_db.unwrap();

        // Assert that the addresses list contains a single empty entry
        assert_eq!(peer_from_db.addresses.addresses().len(), 1);
        // Assert that all other values are empty or zero
        assert_eq!(peer_from_db.addresses.addresses()[0].address(), &Multiaddr::empty());
        assert_eq!(peer_from_db.addresses.addresses()[0].last_seen(), None,);
        assert_eq!(peer_from_db.addresses.addresses()[0].connection_attempts(), 0);
        assert_eq!(peer_from_db.addresses.addresses()[0].avg_initial_dial_time(), None);
        assert_eq!(
            peer_from_db.addresses.addresses()[0].initial_dial_time_sample_count(),
            0
        );
        assert_eq!(peer_from_db.addresses.addresses()[0].avg_latency(), None);
        assert_eq!(peer_from_db.addresses.addresses()[0].latency_sample_count(), 0);
        assert_eq!(peer_from_db.addresses.addresses()[0].last_attempted(), None);
        assert_eq!(peer_from_db.addresses.addresses()[0].last_failed_reason(), None);
        assert_eq!(peer_from_db.addresses.addresses()[0].quality_score(), None);
        assert_eq!(
            peer_from_db.addresses.addresses()[0].source(),
            &PeerAddressSource::Config
        );

        // Assert that the node_id matches
        assert_eq!(peer_from_db.node_id, peer.node_id);

        // Assert that other fields match
        assert_eq!(peer_from_db.public_key, peer.public_key);
        assert_eq!(peer_from_db.features, peer.features);

        // Test 'find_all_peers_match_partial_key'
        for i in 1..NodeId::byte_size() {
            let matches = peers_db
                .find_all_peers_match_partial_key(&peer.node_id.as_bytes()[0..i])
                .unwrap();
            assert!(matches.contains(&peer));
        }
        for i in 1..CommsPublicKey::key_length() {
            let matches = peers_db
                .find_all_peers_match_partial_key(&peer.public_key.as_bytes()[0..i])
                .unwrap();
            assert!(matches.contains(&peer));
        }

        // Verify 'peers::table' has an entry but 'multi_addresses::table' not
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let peer_query: Vec<NewPeerSql> = peers::table.load::<NewPeerSql>(&mut conn).unwrap();
        assert_eq!(peer_query.len(), 1);
        let addresses_query: Vec<NewMultiaddrWithStatsSql> = multi_addresses::table
            .load::<NewMultiaddrWithStatsSql>(&mut conn)
            .unwrap();
        assert!(addresses_query.is_empty());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_get_available_dial_candidates() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap();
        let transport_protocols = TransportProtocol::get_all();

        // Create new node peers each with one known good address
        let mut node_peers = Vec::with_capacity(100);
        for i in 0..100 {
            let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
            if i % 4 == 0 {
                peer.flags = PeerFlags::SEED;
            }
            node_peers.push(peer.clone());
            peers_db.add_or_update_peer(peer).unwrap();
        }

        // Set 'deleted_at' [1]
        peers_db.soft_delete_peer(&node_peers[1].node_id).unwrap();

        // Set 'banned_until' [5]
        peers_db
            .set_banned(
                &node_peers[5].node_id,
                Duration::from_secs(12345),
                "Misbehaviour is punished".to_string(),
            )
            .unwrap();

        // Set 'last_failed_reason' [7]
        for address in node_peers[7].addresses.addresses() {
            peers_db
                .set_last_failed_reason(
                    &node_peers[7].node_id,
                    "not playing with".to_string(),
                    address.address(),
                )
                .unwrap();
        }

        // Pre-amble
        let all_peers = peers_db.get_all_peers(None).unwrap();
        assert_eq!(all_peers.len(), 100);
        let seed_peers = peers_db
            .get_seed_peers()
            .unwrap()
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(seed_peers.len(), 25);

        // Test case 1: Only non-failed peers, no exclusions
        let dial_candidates = peers_db
            .get_available_dial_candidates(
                &[],
                None,
                &transport_protocols,
                true,
                false,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert_eq!(dial_candidates.len(), 97);
        // Verify deleted, banned and failed are not returned
        assert!(!dial_candidates.iter().any(|n| n.node_id == node_peers[1].node_id ||
            n.node_id == node_peers[5].node_id ||
            n.node_id == node_peers[7].node_id));

        // Test case 2: Only non-failed peers, with exclusions
        let dial_candidates = peers_db
            .get_available_dial_candidates(
                &seed_peers,
                None,
                &transport_protocols,
                true,
                false,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert_eq!(dial_candidates.len(), 72);
        // Verify deleted, banned and failed are not returned
        assert!(!dial_candidates.iter().any(|n| n.node_id == node_peers[1].node_id ||
            n.node_id == node_peers[5].node_id ||
            n.node_id == node_peers[7].node_id));
        // Verify excluded are not returned
        for seed_node_id in &seed_peers {
            assert!(!dial_candidates.iter().any(|n| n.node_id == *seed_node_id));
        }

        // Test case 3: Verify randomness
        let limit = 17;
        let mut dial_candidates_1 = peers_db
            .get_available_dial_candidates(
                &[],
                Some(limit),
                &transport_protocols,
                true,
                false,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap()
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(dial_candidates_1.len(), limit);
        dial_candidates_1.sort();

        let mut dial_candidates_2 = peers_db
            .get_available_dial_candidates(
                &[],
                Some(limit),
                &transport_protocols,
                true,
                false,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap()
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(dial_candidates_2.len(), limit);
        dial_candidates_2.sort();

        let mut dial_candidates_3 = peers_db
            .get_available_dial_candidates(
                &[],
                Some(limit),
                &transport_protocols,
                true,
                true,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap()
            .iter()
            .map(|p| p.node_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(dial_candidates_3.len(), limit);
        dial_candidates_3.sort();

        // - With no randomness, results should be the same
        assert_eq!(dial_candidates_1, dial_candidates_2);
        // - With randomness, results should differ
        assert_ne!(dial_candidates_1, dial_candidates_3);
    }

    /// A `seen_since` under which any sighting counts as reached
    fn long_ago() -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::MIN
    }

    fn new_test_db() -> PeerDatabaseSql {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        PeerDatabaseSql::new(
            db_connection,
            &create_test_peer(false, PeerFeatures::COMMUNICATION_NODE),
        )
        .unwrap()
    }

    /// A node peer whose addresses all come from one signed claim, optionally marked as reached, added `age` ago.
    fn node_peer_with_addresses(addresses: Vec<Multiaddr>, seen: bool, age: TimeDelta) -> Peer {
        let mut peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let source = create_peer_address_source_with_claim(addresses.clone(), PeerFeatures::COMMUNICATION_NODE);
        peer.addresses = MultiaddressesWithStats::from_addresses_with_source(addresses.clone(), &source);
        if seen {
            peer.addresses.mark_last_seen_now(&addresses[0]);
        }
        peer.added_at = chrono::Utc::now().naive_utc().checked_sub_signed(age).unwrap();
        peer
    }

    fn stored_node_ids(peers_db: &PeerDatabaseSql) -> Vec<NodeId> {
        let mut node_ids = peers_db
            .get_all_peers(None)
            .unwrap()
            .into_iter()
            .map(|p| p.node_id)
            .collect::<Vec<_>>();
        node_ids.sort();
        node_ids
    }

    #[test]
    fn addresses_round_trip_through_bytes() {
        let peers_db = new_test_db();
        let addresses: Vec<Multiaddr> = vec![
            "/ip4/23.23.23.23/tcp/18189".parse().unwrap(),
            "/ip6/2606:4700:4700::1111/tcp/18189".parse().unwrap(),
            "/dns4/node.example.com/tcp/18189".parse().unwrap(),
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
                .parse()
                .unwrap(),
        ];
        let peer = node_peer_with_addresses(addresses.clone(), true, TimeDelta::zero());
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        let mut stored_addresses = stored.addresses.address_iter().cloned().collect::<Vec<_>>();
        let mut expected = addresses.clone();
        stored_addresses.sort_by_key(|a| a.to_vec());
        expected.sort_by_key(|a| a.to_vec());
        assert_eq!(stored_addresses, expected);
        for address in stored.addresses.iter() {
            let claim = address.source().peer_identity_claim().unwrap();
            assert_eq!(claim.addresses, addresses);
        }
        assert_eq!(peers_db.get_addresses(&peer.node_id).unwrap(), peer.addresses);

        // Updating the peer matches its stored rows on their bytes, so nothing is duplicated
        peers_db.add_or_update_peer(peer.clone()).unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let rows = multi_addresses::table
            .load::<NewMultiaddrWithStatsSql>(&mut conn)
            .unwrap();
        assert_eq!(rows.len(), addresses.len());
        for row in rows {
            let address = Multiaddr::try_from(row.address_bytes).unwrap();
            assert_eq!(row.address, address.to_string());
            assert_eq!(row.netgroup, crate::net_address::address_netgroup(&address));
        }
    }

    /// The stored rows stay readable by an older version, which parses the `address` text column and the claim
    /// addresses in `source` as text.
    #[test]
    fn stored_source_json_keeps_the_old_text_form() {
        let peers_db = new_test_db();
        let plain: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let named: Multiaddr = "/dns4/node.example.com/tcp/18189".parse().unwrap();
        // Text reads back as a different address, and text that does not read back at all
        let lossy = multiaddr::multiaddr!(Dns4("a/tcp/80/ip4/10.0.0.1"), Tcp(1u16));
        let unparsable = multiaddr::multiaddr!(Dns4("evil/x"), Tcp(1u16));
        let peer = node_peer_with_addresses(
            vec![plain.clone(), lossy, named.clone(), unparsable],
            true,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let rows = multi_addresses::table
            .load::<NewMultiaddrWithStatsSql>(&mut conn)
            .unwrap();
        drop(conn);
        // Only the addresses with a lossless text form are stored
        let mut stored_text = rows.iter().map(|row| row.address.clone()).collect::<Vec<_>>();
        stored_text.sort();
        assert_eq!(stored_text, vec![named.to_string(), plain.to_string()]);
        for row in &rows {
            let source: serde_json::Value = serde_json::from_str(&row.source).unwrap();
            assert_eq!(
                source["FromPeerConnection"]["peer_identity_claim"]["addresses"],
                serde_json::json!(["/ip4/23.23.23.23/tcp/18189", "/dns4/node.example.com/tcp/18189"])
            );
            // Exactly what an older version parses: the text column and the claim's addresses as `Multiaddr` text
            row.address.parse::<Multiaddr>().unwrap();
            serde_json::from_str::<PeerAddressSource>(&row.source).unwrap();
        }

        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert_eq!(stored.addresses.len(), 2);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn legacy_poisoned_address_row_is_skipped_not_fatal() {
        use diesel_migrations::MigrationHarness;

        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peers_db = PeerDatabaseSql::new(db_connection.clone(), &this_peer).unwrap();
        let mut good_peers = Vec::new();
        for i in 0..5u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], true, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            good_peers.push(peer);
        }
        let poisoned_address = node_peer_with_addresses(
            vec!["/ip4/24.1.1.1/tcp/18189".parse().unwrap()],
            true,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(poisoned_address.clone()).unwrap();
        let poisoned_claim = node_peer_with_addresses(
            vec!["/ip4/25.1.1.1/tcp/18189".parse().unwrap()],
            true,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(poisoned_claim.clone()).unwrap();
        let lossy_address = node_peer_with_addresses(
            vec!["/ip4/26.1.1.1/tcp/18189".parse().unwrap()],
            true,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(lossy_address.clone()).unwrap();
        // Its own address is fine, but its stored claim also holds a name that is now filtered out
        let bad_sibling = node_peer_with_addresses(
            vec!["/ip4/28.1.1.1/tcp/18189".parse().unwrap()],
            true,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(bad_sibling.clone()).unwrap();
        // A configured address of an unusual shape is the operator's and is kept
        let mut configured = node_peer_with_addresses(vec![], false, TimeDelta::zero());
        let configured_address: Multiaddr = "/ip4/27.1.1.1/udt/sctp/5678".parse().unwrap();
        configured.addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec![configured_address.clone()],
            &PeerAddressSource::Config,
        );
        peers_db.add_or_update_peer(configured.clone()).unwrap();

        // Go back to the schema before this change, where addresses were text only, and write what an older version
        // could have stored from a claim with a `/` in a DNS name
        let mut conn = db_connection.get_pooled_connection().unwrap();
        conn.revert_last_migration(MIGRATIONS).unwrap();
        diesel::sql_query(
            "UPDATE multi_addresses SET address = '/dns4/evil/x/tcp/1' WHERE address = '/ip4/24.1.1.1/tcp/18189'",
        )
        .execute(&mut conn)
        .unwrap();
        // The other shape: the text of `Dns4("a/tcp/80/ip4/10.0.0.1")` parses, as a different, valid multiaddr
        diesel::sql_query(
            "UPDATE multi_addresses SET address = '/dns4/a/tcp/80/ip4/10.0.0.1/tcp/1' WHERE address = \
             '/ip4/26.1.1.1/tcp/18189'",
        )
        .execute(&mut conn)
        .unwrap();
        let source = diesel::sql_query(
            "SELECT source AS value FROM multi_addresses WHERE address = '/ip4/25.1.1.1/tcp/18189'".to_string(),
        )
        .get_result::<TextValue>(&mut conn)
        .unwrap()
        .value;
        let mut source: serde_json::Value = serde_json::from_str(&source).unwrap();
        source["FromPeerConnection"]["peer_identity_claim"]["addresses"] =
            serde_json::json!(["/ip4/25.1.1.1/tcp/18189", "/dns4/evil/x/tcp/1"]);
        diesel::sql_query(format!(
            "UPDATE multi_addresses SET source = '{}' WHERE address = '/ip4/25.1.1.1/tcp/18189'",
            source
        ))
        .execute(&mut conn)
        .unwrap();
        let sibling_source = diesel::sql_query(
            "SELECT source AS value FROM multi_addresses WHERE address = '/ip4/28.1.1.1/tcp/18189'".to_string(),
        )
        .get_result::<TextValue>(&mut conn)
        .unwrap()
        .value;
        let mut sibling_source: serde_json::Value = serde_json::from_str(&sibling_source).unwrap();
        sibling_source["FromPeerConnection"]["peer_identity_claim"]["addresses"] =
            serde_json::json!(["/ip4/28.1.1.1/tcp/18189", "/dns4/a_b.example.com/tcp/1"]);
        diesel::sql_query(format!(
            "UPDATE multi_addresses SET source = '{}' WHERE address = '/ip4/28.1.1.1/tcp/18189'",
            sibling_source
        ))
        .execute(&mut conn)
        .unwrap();
        // A database from before the backfill existed
        diesel::delete(db_metadata::table.filter(db_metadata::key.eq(ADDRESS_BYTES_VERSION_KEY)))
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        // Upgrade: migration, then the data step on open
        db_connection.migrate(MIGRATIONS).unwrap();
        let peers_db = PeerDatabaseSql::new(db_connection, &this_peer).unwrap();

        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let rows = multi_addresses::table
            .load::<NewMultiaddrWithStatsSql>(&mut conn)
            .unwrap();
        // The good rows and the configured one
        assert_eq!(rows.len(), good_peers.len() + 1);
        assert!(
            rows.iter()
                .all(|row| !row.address_bytes.is_empty() && !row.netgroup.is_empty())
        );
        assert!(!rows.iter().any(|row| row.address.starts_with("/dns4/")));
        drop(conn);

        // Every peer is still returned; the four whose only address was poisoned have none left
        let all_peers = peers_db.get_all_peers(None).unwrap();
        assert_eq!(all_peers.len(), good_peers.len() + 5);
        for peer in &all_peers {
            if peer.node_id == configured.node_id {
                assert!(peer.addresses.contains(&configured_address));
            }
            if peer.node_id == poisoned_address.node_id ||
                peer.node_id == poisoned_claim.node_id ||
                peer.node_id == lossy_address.node_id ||
                peer.node_id == bad_sibling.node_id
            {
                assert_eq!(peer.addresses.address_iter().collect::<Vec<_>>(), vec![
                    &Multiaddr::empty()
                ]);
            }
        }
        let candidates = peers_db
            .get_available_dial_candidates(
                &[],
                None,
                &TransportProtocol::get_all(),
                true,
                true,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        let mut candidate_ids = candidates.into_iter().map(|p| p.node_id).collect::<Vec<_>>();
        candidate_ids.sort();
        let mut expected = good_peers.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
        expected.push(configured.node_id.clone());
        expected.sort();
        assert_eq!(candidate_ids, expected);
    }

    /// What an older version does after a downgrade: its diesel schema has no `address_bytes` or `netgroup`, so it
    /// names only the columns it knows on insert and select. The new columns' defaults make that work, and the row is
    /// filled in on the next start of a newer version.
    #[test]
    fn rows_written_by_an_older_version_are_backfilled() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peers_db = PeerDatabaseSql::new(db_connection.clone(), &this_peer).unwrap();
        let address: Multiaddr = "/ip4/23.1.1.1/tcp/18189".parse().unwrap();
        let peer = node_peer_with_addresses(vec![address.clone()], true, TimeDelta::zero());
        peers_db.add_or_update_peer(peer.clone()).unwrap();

        let mut conn = db_connection.get_pooled_connection().unwrap();
        let peer_id = peers::table
            .filter(peers::node_id.eq(peer.node_id.to_hex()))
            .select(peers::peer_id)
            .first::<i64>(&mut conn)
            .unwrap();
        let source = serde_json::to_string(peer.addresses.addresses()[0].source()).unwrap();
        let older: Multiaddr = "/ip4/24.1.1.1/tcp/18189".parse().unwrap();
        diesel::sql_query(format!(
            "INSERT INTO multi_addresses (peer_id, address, last_seen, connection_attempts, avg_initial_dial_time, \
             initial_dial_time_sample_count, avg_latency, latency_sample_count, last_attempted, last_failed_reason, \
             quality_score, source, is_external) VALUES ({peer_id}, '{older}', NULL, 0, NULL, 0, NULL, 0, NULL, NULL, \
             NULL, '{source}', 1)"
        ))
        .execute(&mut conn)
        .unwrap();
        // The older version's reads name only its own columns
        let old_columns =
            diesel::sql_query("SELECT address AS value FROM multi_addresses ORDER BY address_id".to_string())
                .load::<TextValue>(&mut conn)
                .unwrap();
        assert_eq!(old_columns.len(), 2);
        drop(conn);

        let peers_db = PeerDatabaseSql::new(db_connection, &this_peer).unwrap();
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert!(stored.addresses.contains(&address));
        assert!(stored.addresses.contains(&older));
    }

    #[derive(diesel::QueryableByName)]
    struct TextValue {
        #[diesel(sql_type = diesel::sql_types::Text)]
        value: String,
    }

    #[test]
    fn undecodable_address_row_is_skipped_on_read() {
        let peers_db = new_test_db();
        let good: Multiaddr = "/ip4/23.1.1.1/tcp/18189".parse().unwrap();
        let other: Multiaddr = "/ip4/23.2.1.1/tcp/18189".parse().unwrap();
        let peer = node_peer_with_addresses(vec![good.clone(), other.clone()], true, TimeDelta::zero());
        peers_db.add_or_update_peer(peer.clone()).unwrap();
        let healthy = node_peer_with_addresses(vec!["/ip4/24.1.1.1/tcp/1".parse().unwrap()], true, TimeDelta::zero());
        peers_db.add_or_update_peer(healthy.clone()).unwrap();

        // Corrupt one row's bytes and another row's source
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        diesel::update(multi_addresses::table.filter(multi_addresses::address_bytes.eq(other.to_vec())))
            .set(multi_addresses::address_bytes.eq(vec![0xffu8, 0xff, 0xff]))
            .execute(&mut conn)
            .unwrap();
        diesel::update(multi_addresses::table.filter(multi_addresses::address_bytes.eq(good.to_vec())))
            .set(multi_addresses::source.eq("{not json"))
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        assert_eq!(peers_db.get_all_peers(None).unwrap().len(), 2);
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert_eq!(stored.addresses.address_iter().collect::<Vec<_>>(), vec![
            &Multiaddr::empty()
        ]);
        let stored = peers_db.get_peer_by_public_key(&healthy.public_key).unwrap().unwrap();
        assert_eq!(stored.addresses, healthy.addresses);
    }

    /// The cap counts only removable rows: never-seen peers are trimmed to it, in random order, while reached peers are
    /// kept and do not count. (The name predates random eviction order.)
    #[test]
    fn peer_row_cap_evicts_never_seen_oldest_first() {
        let peers_db = new_test_db();
        let mut seen = Vec::new();
        let mut unseen = Vec::new();
        for i in 0..10u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            // The seen peers are the oldest, so age alone would remove them first
            let is_seen = i < 3;
            let peer = node_peer_with_addresses(vec![address], is_seen, TimeDelta::hours(100 - i64::from(i)));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            if is_seen {
                seen.push(peer.node_id);
            } else {
                unseen.push(peer.node_id);
            }
        }

        let stats = peers_db.garbage_collect(5, &[], long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats {
            excess_peers_removed: 2,
            banned_peers_removed: 0,
            address_less_peers_removed: 0,
            soft_deleted_peers_removed: 0,
        });
        let stored = stored_node_ids(&peers_db);
        assert!(seen.iter().all(|id| stored.contains(id)));
        assert_eq!(unseen.iter().filter(|id| stored.contains(id)).count(), 5);
        assert_eq!(peers_db.size(), 8);

        // At the cap there is nothing more to do
        assert_eq!(
            peers_db.garbage_collect(5, &[], long_ago()).unwrap(),
            GarbageCollectionStats::default()
        );
    }

    #[test]
    fn garbage_collection_keeps_banned_seed_and_config_peers() {
        let peers_db = new_test_db();
        let old = TimeDelta::hours(100);
        let address = |i: u8| format!("/ip4/23.{i}.1.1/tcp/18189").parse::<Multiaddr>().unwrap();

        let banned = node_peer_with_addresses(vec![address(1)], false, old);
        peers_db.add_or_update_peer(banned.clone()).unwrap();
        peers_db
            .set_banned(&banned.node_id, Duration::from_secs(3600), "bad".to_string())
            .unwrap();
        let mut seed = node_peer_with_addresses(vec![address(2)], false, old);
        seed.flags = PeerFlags::SEED;
        peers_db.add_or_update_peer(seed.clone()).unwrap();
        let mut configured = node_peer_with_addresses(vec![], false, old);
        configured.addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![address(3)], &PeerAddressSource::Config);
        peers_db.add_or_update_peer(configured.clone()).unwrap();
        let expendable = node_peer_with_addresses(vec![address(4)], false, TimeDelta::zero());
        peers_db.add_or_update_peer(expendable).unwrap();

        // Cap of 5: everything fits, the banned row within its own cap of 5 / 5 = 1
        let stats = peers_db.garbage_collect(5, &[], long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats::default());
        // Cap of zero: everything removable goes (including the banned row, whose own cap is now 0), seeds and
        // configured peers stay
        let stats = peers_db.garbage_collect(0, &[], long_ago()).unwrap();
        assert_eq!(stats.excess_peers_removed, 1);
        assert_eq!(stats.banned_peers_removed, 1);
        let mut expected = vec![seed.node_id, configured.node_id];
        expected.sort();
        assert_eq!(stored_node_ids(&peers_db), expected);

        // Soft-deleted peers go after a day, unless banned
        let banned = node_peer_with_addresses(vec![address(1)], false, old);
        peers_db.add_or_update_peer(banned.clone()).unwrap();
        peers_db
            .set_banned(&banned.node_id, Duration::from_secs(3600), "bad".to_string())
            .unwrap();
        let soft_deleted = node_peer_with_addresses(vec![address(5)], true, TimeDelta::zero());
        peers_db.add_or_update_peer(soft_deleted.clone()).unwrap();
        let recently_deleted = node_peer_with_addresses(vec![address(6)], true, TimeDelta::zero());
        peers_db.add_or_update_peer(recently_deleted.clone()).unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let two_days_ago = chrono::Utc::now().naive_utc() - TimeDelta::days(2);
        diesel::update(
            peers::table.filter(peers::node_id.eq_any(vec![soft_deleted.node_id.to_hex(), banned.node_id.to_hex()])),
        )
        .set(peers::deleted_at.eq(two_days_ago))
        .execute(&mut conn)
        .unwrap();
        drop(conn);
        peers_db.soft_delete_peer(&recently_deleted.node_id).unwrap();

        let stats = peers_db.garbage_collect(100, &[], long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats {
            excess_peers_removed: 0,
            banned_peers_removed: 0,
            address_less_peers_removed: 0,
            soft_deleted_peers_removed: 1,
        });
        assert!(peers_db.get_peer_by_node_id(&soft_deleted.node_id).unwrap().is_none());
        assert!(
            peers_db
                .get_peer_by_node_id(&recently_deleted.node_id)
                .unwrap()
                .is_some()
        );
        assert!(peers_db.get_peer_by_node_id(&banned.node_id).unwrap().is_some());
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let orphaned_addresses = multi_addresses::table
            .filter(multi_addresses::address_bytes.eq(address(5).to_vec()))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap();
        assert_eq!(orphaned_addresses, 0);
    }

    /// A flood of keys that are stored and then banned must not push out never-seen honest peers: banned rows have
    /// their own cap and do not count against `max_peer_rows`.
    #[test]
    fn banned_rows_with_addresses_are_capped_separately() {
        let peers_db = new_test_db();
        let max_peer_rows = 10;
        let mut honest = Vec::new();
        for i in 0..10u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::hours(100));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            honest.push(peer.node_id);
        }
        let mut banned = Vec::new();
        for i in 0..12u64 {
            let address = format!("/ip4/24.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            peers_db
                .set_banned(&peer.node_id, Duration::from_secs(3600 + i * 60), "bad".to_string())
                .unwrap();
            banned.push(peer.node_id);
        }

        let stats = peers_db.garbage_collect(max_peer_rows, &[], long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats {
            excess_peers_removed: 0,
            // 12 rows, cap 10 / 5 = 2; the bans ending soonest go first
            banned_peers_removed: 10,
            address_less_peers_removed: 0,
            soft_deleted_peers_removed: 0,
        });
        let mut expected = honest.clone();
        expected.extend(banned[10..].iter().cloned());
        expected.sort();
        assert_eq!(stored_node_ids(&peers_db), expected);
    }

    /// A ban "forever" used to be stored as `+262142-...`, which compares as text before every current time, so the
    /// peer read as unbanned to SQL: it stayed a dial candidate and garbage collection removed it, losing the ban.
    #[test]
    fn indefinite_bans_stay_in_force_in_sql() {
        let peers_db = new_test_db();
        let forever = Duration::from_secs(u64::MAX);
        let address = |i: u8| format!("/ip4/23.{i}.1.1/tcp/18189").parse::<Multiaddr>().unwrap();

        // With addresses, never reached: banned forever, and banned for an hour
        let banned_forever = node_peer_with_addresses(vec![address(1)], false, TimeDelta::hours(100));
        peers_db.add_or_update_peer(banned_forever.clone()).unwrap();
        peers_db
            .set_banned(&banned_forever.node_id, forever, "forever".to_string())
            .unwrap();
        let banned_briefly = node_peer_with_addresses(vec![address(2)], false, TimeDelta::hours(100));
        peers_db.add_or_update_peer(banned_briefly.clone()).unwrap();
        peers_db
            .set_banned(&banned_briefly.node_id, Duration::from_secs(3600), "brief".to_string())
            .unwrap();
        // Without addresses (handshake bans): forever, and for an hour
        let forever_key = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE).public_key;
        let address_less_forever = peers_db
            .ban_or_add_banned_peer(&forever_key, forever, "forever".to_string())
            .unwrap();
        let brief_key = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE).public_key;
        let address_less_brief = peers_db
            .ban_or_add_banned_peer(&brief_key, Duration::from_secs(3600), "brief".to_string())
            .unwrap();

        // Read back, it is still a ban forever
        let stored = peers_db.get_peer_by_node_id(&banned_forever.node_id).unwrap().unwrap();
        assert!(stored.is_banned());
        assert!(crate::utils::datetime::is_max_datetime(stored.banned_until().unwrap()));

        // Not a dial candidate
        let candidates = peers_db
            .get_available_dial_candidates(
                &[],
                None,
                &TransportProtocol::get_all(),
                true,
                true,
                0,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert!(candidates.is_empty(), "{candidates:?}");

        // Never-seen cap 0 does not take it (it is banned); banned and address-less caps of 1 (5 / 5) keep it over the
        // timed bans
        let stats = peers_db.garbage_collect(5, &[], long_ago()).unwrap();
        assert_eq!(stats.excess_peers_removed, 0);
        assert_eq!(stats.banned_peers_removed, 1);
        assert_eq!(stats.address_less_peers_removed, 1);
        assert!(peers_db.get_peer_by_node_id(&banned_forever.node_id).unwrap().is_some());
        assert!(peers_db.get_peer_by_node_id(&banned_briefly.node_id).unwrap().is_none());
        assert!(peers_db.get_peer_by_node_id(&address_less_forever).unwrap().is_some());
        assert!(peers_db.get_peer_by_node_id(&address_less_brief).unwrap().is_none());

        // Soft deleted long ago, an active ban still keeps the row
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        diesel::update(peers::table.filter(peers::node_id.eq(banned_forever.node_id.to_hex())))
            .set(peers::deleted_at.eq(chrono::Utc::now().naive_utc() - TimeDelta::days(2)))
            .execute(&mut conn)
            .unwrap();
        drop(conn);
        let stats = peers_db.garbage_collect(5, &[], long_ago()).unwrap();
        assert_eq!(stats.soft_deleted_peers_removed, 0);
        assert!(peers_db.get_peer_by_node_id(&banned_forever.node_id).unwrap().is_some());
    }

    #[test]
    fn stored_indefinite_ban_times_are_repaired_on_open() {
        let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peers_db = PeerDatabaseSql::new(db_connection.clone(), &this_peer).unwrap();
        let live = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(live.clone()).unwrap();
        let soft_deleted = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(soft_deleted.clone()).unwrap();
        peers_db.soft_delete_peer(&soft_deleted.node_id).unwrap();

        // What an earlier build wrote for a ban forever
        let mut conn = db_connection.get_pooled_connection().unwrap();
        diesel::sql_query(
            "UPDATE peers SET banned_until = '+262142-12-31 23:59:59.999999999' WHERE node_id IN ('".to_string() +
                &live.node_id.to_hex() +
                "', '" +
                &soft_deleted.node_id.to_hex() +
                "')",
        )
        .execute(&mut conn)
        .unwrap();
        drop(conn);

        let peers_db = PeerDatabaseSql::new(db_connection, &this_peer).unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let stored = diesel::sql_query(
            "SELECT banned_until AS value FROM peers WHERE banned_until IS NOT NULL ORDER BY node_id".to_string(),
        )
        .load::<TextValue>(&mut conn)
        .unwrap();
        drop(conn);
        assert_eq!(stored.len(), 2);
        for row in stored {
            assert_eq!(row.value, "9999-12-31 23:59:59");
        }
        assert!(
            peers_db
                .get_peer_by_node_id(&live.node_id)
                .unwrap()
                .unwrap()
                .is_banned()
        );
        assert!(
            peers_db
                .get_peer_by_node_id(&soft_deleted.node_id)
                .unwrap()
                .unwrap()
                .is_banned()
        );
    }

    /// `banned_until` stays set after a ban expires. A peer whose ban has expired is judged like any other peer: kept
    /// while recently reached, and never-seen otherwise, instead of being first out of the banned class.
    #[test]
    fn expired_bans_are_not_in_the_banned_class() {
        let peers_db = new_test_db();
        let address = |i: u8| format!("/ip4/23.{i}.1.1/tcp/18189").parse::<Multiaddr>().unwrap();
        let expire_ban = |node_id: &NodeId| {
            let mut conn = peers_db.connection.get_pooled_connection().unwrap();
            diesel::update(peers::table.filter(peers::node_id.eq(node_id.to_hex())))
                .set(peers::banned_until.eq(chrono::Utc::now().naive_utc() - TimeDelta::hours(1)))
                .execute(&mut conn)
                .unwrap();
        };

        // Banned once, for a short while, and reached since
        let reached = node_peer_with_addresses(vec![address(1)], true, TimeDelta::hours(100));
        peers_db.add_or_update_peer(reached.clone()).unwrap();
        peers_db
            .set_banned(&reached.node_id, Duration::from_secs(1800), "sync".to_string())
            .unwrap();
        expire_ban(&reached.node_id);
        // Banned once and never reached
        let unreached = node_peer_with_addresses(vec![address(2)], false, TimeDelta::hours(100));
        peers_db.add_or_update_peer(unreached.clone()).unwrap();
        peers_db
            .set_banned(&unreached.node_id, Duration::from_secs(1800), "sync".to_string())
            .unwrap();
        expire_ban(&unreached.node_id);
        // A flood of active long bans, far over the banned cap of 10 / 5 = 2
        let mut active = Vec::new();
        for i in 10..20u8 {
            let peer = node_peer_with_addresses(vec![address(i)], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            peers_db
                .set_banned(&peer.node_id, Duration::from_secs(30 * 24 * 3600), "bad".to_string())
                .unwrap();
            active.push(peer.node_id);
        }

        let stats = peers_db.garbage_collect(10, &[], long_ago()).unwrap();
        // Only active bans are in the banned class, and they are trimmed to its cap
        assert_eq!(stats.banned_peers_removed, 8);
        assert!(peers_db.get_peer_by_node_id(&reached.node_id).unwrap().is_some());
        assert!(peers_db.get_peer_by_node_id(&unreached.node_id).unwrap().is_some());

        // The unreached one counts as never-seen: with room for none, it goes, the reached one stays
        let stats = peers_db.garbage_collect(0, &[], long_ago()).unwrap();
        assert_eq!(stats.excess_peers_removed, 1);
        assert!(peers_db.get_peer_by_node_id(&unreached.node_id).unwrap().is_none());
        assert!(peers_db.get_peer_by_node_id(&reached.node_id).unwrap().is_some());
    }

    /// A sighting older than the expiry no longer protects a peer from garbage collection or puts it first in line to
    /// be dialled.
    #[test]
    fn expired_last_seen_no_longer_counts_as_reached() {
        let peers_db = new_test_db();
        let transport_protocols = TransportProtocol::get_all();
        let address: Multiaddr = "/ip4/23.1.1.1/tcp/18189".parse().unwrap();
        let stale = node_peer_with_addresses(vec![address.clone()], false, TimeDelta::hours(100));
        peers_db.add_or_update_peer(stale.clone()).unwrap();
        let two_days_ago = chrono::Utc::now().naive_utc() - TimeDelta::days(2);
        peers_db.set_last_seen(&stale.node_id, two_days_ago, &address).unwrap();
        let mut fresh = Vec::new();
        for i in 2..12u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            fresh.push(peer.node_id);
        }
        let seen_since = chrono::Utc::now().naive_utc() - TimeDelta::days(1);

        // Not ranked first: it is one of eleven unreached peers, so it is rarely first
        let mut first_count = 0;
        for _ in 0..50 {
            let candidates = peers_db
                .get_available_dial_candidates(
                    &[],
                    None,
                    &transport_protocols,
                    true,
                    true,
                    4,
                    &HashMap::new(),
                    seen_since,
                )
                .unwrap();
            if candidates[0].node_id == stale.node_id {
                first_count += 1;
            }
        }
        assert!(first_count < 25, "{first_count}");
        // With any sighting counting, it would always be first
        let candidates = peers_db
            .get_available_dial_candidates(
                &[],
                None,
                &transport_protocols,
                true,
                true,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert_eq!(candidates[0].node_id, stale.node_id);

        // Removable: with room for only ten never-seen peers, and the stale one counted among them, one goes. With the
        // old sighting counting, it would be kept and the ten fresh ones would fit.
        assert_eq!(
            peers_db.garbage_collect(10, &[], long_ago()).unwrap(),
            GarbageCollectionStats::default()
        );
        let stats = peers_db.garbage_collect(10, &[], seen_since).unwrap();
        assert_eq!(stats.excess_peers_removed, 1);
    }

    #[test]
    fn ban_only_rows_are_capped_separately_and_do_not_push_out_peers() {
        let peers_db = new_test_db();
        let max_peer_rows = 10;
        // Honest peers we have never reached, at the row cap
        let mut honest = Vec::new();
        for i in 0..10u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::hours(100));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            honest.push(peer.node_id);
        }
        // A flood of handshake bans for fresh keys, far past both caps. The ban ending soonest goes first.
        let mut bans = Vec::new();
        for i in 0..12u64 {
            let key = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE).public_key;
            let node_id = peers_db
                .ban_or_add_banned_peer(&key, Duration::from_secs(3600 + i * 60), "bad".to_string())
                .unwrap();
            bans.push(node_id);
        }
        // One whose ban already ended goes before any active one
        let expired_key = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE).public_key;
        let expired = peers_db
            .ban_or_add_banned_peer(&expired_key, Duration::from_secs(3600), "bad".to_string())
            .unwrap();
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        diesel::update(peers::table.filter(peers::node_id.eq(expired.to_hex())))
            .set(peers::banned_until.eq(chrono::Utc::now().naive_utc().checked_sub_signed(TimeDelta::hours(1))))
            .execute(&mut conn)
            .unwrap();
        drop(conn);

        let stats = peers_db.garbage_collect(max_peer_rows, &[], long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats {
            excess_peers_removed: 0,
            banned_peers_removed: 0,
            // 13 rows, cap 10 / 5 = 2
            address_less_peers_removed: 11,
            soft_deleted_peers_removed: 0,
        });
        let mut expected = honest.clone();
        // The two longest bans are kept
        expected.extend(bans[10..].iter().cloned());
        expected.sort();
        assert_eq!(stored_node_ids(&peers_db), expected);
        assert!(peers_db.get_peer_by_node_id(&expired).unwrap().is_none());
    }

    #[test]
    fn ban_reasons_are_stored_sanitised() {
        let peers_db = new_test_db();
        let peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db.add_or_update_peer(peer.clone()).unwrap();
        let reason = format!("bad\u{202e}\nFAKE LOG{}", "x".repeat(1000));
        peers_db
            .set_banned(&peer.node_id, Duration::from_secs(3600), reason.clone())
            .unwrap();
        let stored = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().unwrap();
        assert!(stored.banned_reason.starts_with("badFAKE LOG"));
        assert_eq!(stored.banned_reason.len(), 256);

        let unknown = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        peers_db
            .ban_or_add_banned_peer(&unknown.public_key, Duration::from_secs(3600), reason)
            .unwrap();
        let stored = peers_db.get_peer_by_public_key(&unknown.public_key).unwrap().unwrap();
        assert!(stored.banned_reason.starts_with("badFAKE LOG"));
        assert_eq!(stored.banned_reason.len(), 256);
    }

    #[test]
    fn garbage_collection_never_removes_connected_peers() {
        let peers_db = new_test_db();
        // A long-connected inbound neighbour: its claimed address was never dialled, so never seen
        let inbound = node_peer_with_addresses(
            vec!["/ip4/23.0.1.1/tcp/18189".parse().unwrap()],
            false,
            TimeDelta::hours(100),
        );
        peers_db.add_or_update_peer(inbound.clone()).unwrap();
        let mut unconnected = Vec::new();
        for i in 1..8u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::hours(100 - i64::from(i)));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            unconnected.push(peer.node_id);
        }
        // A connected wallet with no address, the oldest of the address-less rows
        let wallet = node_peer_with_addresses(vec![], false, TimeDelta::hours(50));
        peers_db.add_or_update_peer(wallet.clone()).unwrap();
        let mut address_less = Vec::new();
        for hours in [40, 10, 30, 20] {
            let peer = node_peer_with_addresses(vec![], false, TimeDelta::hours(hours));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            address_less.push((hours, peer.node_id));
        }
        let connected = vec![inbound.node_id.clone(), wallet.node_id.clone()];

        // Address-less cap 15 / 5 = 3, and the connected wallet does not count: of the 4 unconnected, the oldest goes
        // (not by row id)
        let stats = peers_db.garbage_collect(15, &connected, long_ago()).unwrap();
        assert_eq!(stats.address_less_peers_removed, 1);
        for (hours, node_id) in &address_less {
            let kept = peers_db.get_peer_by_node_id(node_id).unwrap().is_some();
            assert_eq!(kept, *hours < 40, "{hours}h");
        }

        // Both caps exceeded: unconnected rows are trimmed, the connected ones stay
        let stats = peers_db.garbage_collect(5, &connected, long_ago()).unwrap();
        assert_eq!(stats, GarbageCollectionStats {
            excess_peers_removed: 2,
            banned_peers_removed: 0,
            address_less_peers_removed: 2,
            soft_deleted_peers_removed: 0,
        });
        assert!(peers_db.get_peer_by_node_id(&inbound.node_id).unwrap().is_some());
        assert!(peers_db.get_peer_by_node_id(&wallet.node_id).unwrap().is_some());
        let stored = stored_node_ids(&peers_db);
        assert_eq!(unconnected.iter().filter(|id| stored.contains(id)).count(), 5);
        assert_eq!(address_less.iter().filter(|(_, id)| stored.contains(id)).count(), 1);
        assert_eq!(stored.len(), 8);
    }

    /// Candidates are read without a write lock, so a peer can change between the read and its delete. The delete
    /// re-checks every condition.
    #[test]
    fn garbage_collection_rechecks_candidates_before_deleting() {
        let peers_db = new_test_db();
        let mut peers = Vec::new();
        for i in 0..4u8 {
            let address = format!("/ip4/23.{i}.1.1/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::hours(10 - i64::from(i)));
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            peers.push(peer);
        }
        let now = chrono::Utc::now().naive_utc();
        let context = GarbageCollectionContext {
            now,
            deleted_before: now.checked_sub_signed(TimeDelta::hours(24)).unwrap(),
            seen_since: long_ago(),
            config_source: serde_json::to_string(&PeerAddressSource::Config).unwrap(),
            connected_hex: vec![],
        };
        let mut conn = peers_db.connection.get_pooled_connection().unwrap();
        let candidates = garbage_collection_candidates(GarbageCollectionPass::NeverSeen, &context)
            .load::<i64>(&mut conn)
            .unwrap();
        assert_eq!(candidates.len(), 4);

        // Between the read and the delete: one is reached, one is banned, one gets a live connection
        let reached = &peers[0];
        peers_db
            .set_last_seen(&reached.node_id, now, reached.addresses.addresses()[0].address())
            .unwrap();
        peers_db
            .set_banned(&peers[1].node_id, Duration::from_secs(3600), "bad".to_string())
            .unwrap();
        let context = GarbageCollectionContext {
            connected_hex: vec![peers[2].node_id.to_hex()],
            ..context
        };

        let deleted = PeerDatabaseSql::delete_if_still_eligible(
            &mut conn,
            GarbageCollectionPass::NeverSeen,
            &context,
            &candidates,
        )
        .unwrap();
        drop(conn);
        assert_eq!(deleted, 1);
        for (i, peer) in peers.iter().enumerate() {
            let kept = peers_db.get_peer_by_node_id(&peer.node_id).unwrap().is_some();
            assert_eq!(kept, i < 3, "peer {i}");
        }
    }

    #[test]
    fn multi_address_peers_do_not_bypass_the_netgroup_cap() {
        let peers_db = new_test_db();
        let transport_protocols = TransportProtocol::get_all();
        // Each Sybil claims an address in one crowded /16 plus a DNS name of its own (free with wildcard DNS)
        let mut sybils = Vec::new();
        for i in 0..30u8 {
            let addresses = vec![
                format!("/ip4/23.23.{i}.1/tcp/18189").parse().unwrap(),
                format!("/dns4/s{i}.attacker.example/tcp/18189").parse().unwrap(),
            ];
            let peer = node_peer_with_addresses(addresses, false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            sybils.push(peer.node_id);
        }
        let mut honest = Vec::new();
        for i in 0..5u8 {
            let address = format!("/ip4/{}.1.1.1/tcp/18189", 30 + i).parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            honest.push(peer.node_id);
        }

        for randomize in [true, false] {
            for _ in 0..10 {
                let candidates = peers_db
                    .get_available_dial_candidates(
                        &[],
                        None,
                        &transport_protocols,
                        true,
                        randomize,
                        4,
                        &HashMap::new(),
                        long_ago(),
                    )
                    .unwrap()
                    .into_iter()
                    .map(|p| p.node_id)
                    .collect::<Vec<_>>();
                let from_sybils = candidates.iter().filter(|id| sybils.contains(id)).count();
                assert!(from_sybils <= 4, "{from_sybils}");
                assert!(honest.iter().all(|id| candidates.contains(id)));
            }
        }
    }

    #[test]
    fn wildcard_dns_sybils_are_one_netgroup() {
        let peers_db = new_test_db();
        // No transport filter: none of the transport prefixes match DNS addresses
        let transport_protocols: Vec<TransportProtocol> = vec![];
        let mut sybils = Vec::new();
        for i in 1..=20u8 {
            let address = format!("/dns4/a{i}.evil.com/tcp/18189").parse().unwrap();
            let peer = node_peer_with_addresses(vec![address], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            sybils.push(peer.node_id);
        }
        let honest = node_peer_with_addresses(
            vec!["/dns4/node.example.com/tcp/18189".parse().unwrap()],
            false,
            TimeDelta::zero(),
        );
        peers_db.add_or_update_peer(honest.clone()).unwrap();

        for _ in 0..10 {
            let candidates = peers_db
                .get_available_dial_candidates(
                    &[],
                    None,
                    &transport_protocols,
                    true,
                    true,
                    4,
                    &HashMap::new(),
                    long_ago(),
                )
                .unwrap();
            assert_eq!(candidates.iter().filter(|p| sybils.contains(&p.node_id)).count(), 4);
            assert!(candidates.iter().any(|p| p.node_id == honest.node_id));
        }
    }

    #[test]
    fn dial_candidates_come_back_in_selection_order() {
        let peers_db = new_test_db();
        let transport_protocols = TransportProtocol::get_all();
        let mut seen = Vec::new();
        for i in 0..20u8 {
            let address = format!("/ip4/{}.1.1.1/tcp/18189", 30 + i).parse().unwrap();
            let is_seen = i % 4 == 0;
            let peer = node_peer_with_addresses(vec![address], is_seen, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            if is_seen {
                seen.push(peer.node_id);
            }
        }
        for _ in 0..10 {
            let candidates = peers_db
                .get_available_dial_candidates(
                    &[],
                    None,
                    &transport_protocols,
                    true,
                    true,
                    4,
                    &HashMap::new(),
                    long_ago(),
                )
                .unwrap();
            assert_eq!(candidates.len(), 20);
            // Previously reached peers lead the list
            for peer in candidates.iter().take(seen.len()) {
                assert!(seen.contains(&peer.node_id));
            }
        }
    }

    #[test]
    fn dial_candidates_capped_per_netgroup() {
        let peers_db = new_test_db();
        let transport_protocols = TransportProtocol::get_all();
        let mut crowded = Vec::new();
        let mut crowded_seen = Vec::new();
        for i in 0..20u8 {
            let address = format!("/ip4/23.23.{i}.1/tcp/18189").parse().unwrap();
            let is_seen = i < 2;
            let peer = node_peer_with_addresses(vec![address], is_seen, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            if is_seen {
                crowded_seen.push(peer.node_id.clone());
            }
            crowded.push(peer.node_id);
        }
        let mut elsewhere = Vec::new();
        for address in ["/ip4/24.1.1.1/tcp/18189", "/ip6/2606:4700::1/tcp/18189"] {
            let peer = node_peer_with_addresses(vec![address.parse().unwrap()], false, TimeDelta::zero());
            peers_db.add_or_update_peer(peer.clone()).unwrap();
            elsewhere.push(peer.node_id);
        }

        for randomize in [true, false] {
            for _ in 0..10 {
                let candidates = peers_db
                    .get_available_dial_candidates(
                        &[],
                        None,
                        &transport_protocols,
                        true,
                        randomize,
                        4,
                        &HashMap::new(),
                        long_ago(),
                    )
                    .unwrap()
                    .into_iter()
                    .map(|p| p.node_id)
                    .collect::<Vec<_>>();
                let from_crowded = candidates.iter().filter(|id| crowded.contains(id)).count();
                assert_eq!(from_crowded, 4);
                // Addresses we reached before are taken first
                assert!(crowded_seen.iter().all(|id| candidates.contains(id)));
                assert!(elsewhere.iter().all(|id| candidates.contains(id)));
                assert_eq!(candidates.len(), 6);
            }
        }

        // A limit is filled from previously reached addresses first
        let candidates = peers_db
            .get_available_dial_candidates(
                &[],
                Some(2),
                &transport_protocols,
                true,
                true,
                4,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert!(candidates.iter().all(|p| crowded_seen.contains(&p.node_id)));

        // A top-up selection only gets what is left of each group's budget
        let mut netgroups_taken = HashMap::new();
        netgroups_taken.insert("ip4:23.23".to_string(), 3usize);
        for _ in 0..10 {
            let candidates = peers_db
                .get_available_dial_candidates(
                    &[],
                    None,
                    &transport_protocols,
                    true,
                    true,
                    4,
                    &netgroups_taken,
                    long_ago(),
                )
                .unwrap();
            let from_crowded = candidates.iter().filter(|p| crowded.contains(&p.node_id)).count();
            assert_eq!(from_crowded, 1);
            assert_eq!(candidates.len(), 3);
        }

        // 0 disables the cap
        let candidates = peers_db
            .get_available_dial_candidates(
                &[],
                None,
                &transport_protocols,
                true,
                true,
                0,
                &HashMap::new(),
                long_ago(),
            )
            .unwrap();
        assert_eq!(candidates.len(), 22);
    }
}
