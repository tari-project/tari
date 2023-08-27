//  Copyright 2020, The Taiji Project
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

use taiji_storage::lmdb_store::{LMDBDatabase, LMDBError};

/// No structural changes, removes peers with onionv2 addresses
pub struct Migration;

impl super::Migration<LMDBDatabase> for Migration {
    type Error = LMDBError;

    fn get_version(&self) -> u32 {
        7
    }

    fn migrate(&self, _db: &LMDBDatabase) -> Result<(), Self::Error> {
        // Kept here as an example...

        // db.for_each::<PeerId, Peer, _>(|old_peer| {
        //     let result = old_peer.and_then(|(key, peer)| {
        //         if key == MIGRATION_VERSION_KEY {
        //             return Ok(());
        //         }
        //         if peer.addresses.iter().any(|a| {
        //             // Starts with /onion/
        //             a.iter()
        //                 .next()
        //                 .map(|p| matches!(p, multiaddr::Protocol::Onion(_, _)))
        //                 .unwrap_or(false)
        //         }) {
        //             debug!(
        //                 target: LOG_TARGET,
        //                 "Removing onionv2 peer `{}`",
        //                 peer.node_id.short_str()
        //             );
        //             db.remove(&key)?;
        //         }
        //
        //         Ok(())
        //     });
        //
        //     if let Err(err) = result {
        //         error!(
        //             target: LOG_TARGET,
        //             "Failed to deserialize peer: {} ** Database may be corrupt **", err
        //         );
        //     }
        //     IterationResult::Continue
        // })?;

        Ok(())
    }
}
