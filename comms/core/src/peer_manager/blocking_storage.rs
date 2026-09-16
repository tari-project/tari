// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Async-safe access to the (synchronous) peer database.
//!
//! Every peer database call ultimately runs diesel/r2d2 against SQLite, which is *blocking*: it
//! waits on an r2d2 pool checkout and then on SQLite's own single-writer lock. Calling that
//! directly from an `async fn` does not park a task, it parks the tokio **worker thread** running
//! that task. Under sustained peer-database write pressure the worker pool drains one thread at a
//! time until the comms actors (`ConnectionManager`, `Dialer`, `ConnectivityManager`) stop being
//! scheduled at all - the node keeps its TCP connections but can no longer dial or propagate.
//!
//! [`BlockingPeerStorage`] exists to make that mistake unrepresentable: it owns the only
//! [`PeerStorageSql`] handle, never hands it out, and the sole way to reach it is
//! [`BlockingPeerStorage::call`], which moves the work onto `tokio::task::spawn_blocking`.

use tokio::task;

use crate::peer_manager::{PeerManagerError, ThisPeerIdentity, peer_storage_sql::PeerStorageSql};

/// Owns the peer database handle and only ever touches it from the blocking thread pool.
///
/// The inner [`PeerStorageSql`] is deliberately private with no accessor. Adding a synchronous peer
/// database call to [`PeerManager`](crate::peer_manager::PeerManager) therefore does not compile
/// unless it goes through [`Self::call`].
#[derive(Clone)]
pub(super) struct BlockingPeerStorage {
    storage: PeerStorageSql,
}

impl BlockingPeerStorage {
    pub(super) fn new(storage: PeerStorageSql) -> Self {
        Self { storage }
    }

    /// This node's own identity.
    ///
    /// The single exception to the "no synchronous access" rule: the identity is captured in memory
    /// when the database handle is constructed and reading it performs no I/O at all.
    pub(super) fn this_peer_identity(&self) -> ThisPeerIdentity {
        self.storage.this_peer_identity()
    }

    /// Run `f` against the peer database on tokio's blocking thread pool and await the result.
    ///
    /// The caller's worker thread stays free while the query waits on the r2d2 pool and the SQLite
    /// lock. Note that dropping the returned future (for example because the caller wrapped it in
    /// `tokio::time::timeout`) does not cancel `f` - it runs to completion on the blocking pool,
    /// which is what makes a timed-out lookup safe to shed.
    pub(super) async fn call<F, R>(&self, f: F) -> Result<R, PeerManagerError>
    where
        F: FnOnce(&PeerStorageSql) -> Result<R, PeerManagerError> + Send + 'static,
        R: Send + 'static,
    {
        let storage = self.storage.clone();
        task::spawn_blocking(move || f(&storage)).await?
    }
}
