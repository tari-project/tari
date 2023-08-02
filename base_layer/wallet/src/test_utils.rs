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

use core::iter;
use std::path::Path;

use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use tari_common::configuration::Network;
use tari_core::consensus::{ConsensusConstants, ConsensusManager};
use tempfile::{tempdir, TempDir};

use crate::storage::sqlite_utilities::{
    run_migration_and_create_sqlite_connection,
    wallet_db_connection::WalletDbConnection,
};

pub fn random_string(len: usize) -> String {
    iter::repeat(())
        .map(|_| OsRng.sample(Alphanumeric) as char)
        .take(len)
        .collect()
}

/// A test helper to create a temporary wallet service databases
pub fn make_wallet_database_connection(path: Option<String>) -> (WalletDbConnection, Option<TempDir>) {
    let (path_string, temp_dir): (String, Option<TempDir>) = if let Some(p) = path {
        (p, None)
    } else {
        let temp_dir = tempdir().unwrap();
        let path_string = temp_dir.path().to_str().unwrap().to_string();
        (path_string, Some(temp_dir))
    };

    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_path = Path::new(&path_string).join(db_name);

    let connection =
        run_migration_and_create_sqlite_connection(db_path.to_str().expect("Should be able to make path"), 16).unwrap();
    (connection, temp_dir)
}

pub fn create_consensus_rules() -> ConsensusManager {
    ConsensusManager::builder(Network::LocalNet).build().unwrap()
}

pub fn create_consensus_constants(height: u64) -> ConsensusConstants {
    create_consensus_rules().consensus_constants(height).clone()
}
