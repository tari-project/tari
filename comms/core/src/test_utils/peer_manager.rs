//  Copyright 2022, The Tari Project
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

use std::{iter, sync::Arc};

use rand::{distributions::Alphanumeric, Rng};
use tari_common_sqlite::connection::DbConnection;

#[cfg(test)]
use crate::peer_manager::{Peer, PeerManagerError};
use crate::{
    peer_manager::database::{PeerDatabaseSql, MIGRATIONS},
    PeerManager,
};

#[cfg(test)]
pub fn build_peer_manager(this_peer: &Peer) -> Result<Arc<PeerManager>, PeerManagerError> {
    let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS)?;
    let peers_db = PeerDatabaseSql::new(db_connection, this_peer)?;
    Ok(Arc::new(PeerManager::new(peers_db)?))
}

#[cfg(not(test))]
pub use not_test::build_peer_manager;

#[cfg(not(test))]
mod not_test {
    use std::path::{Path, PathBuf};

    use tari_common_sqlite::connection::DbConnectionUrl;

    use super::*;
    use crate::peer_manager::{Peer, PeerManagerError};

    pub fn build_peer_manager<P: AsRef<Path>>(
        data_path: P,
        this_peer: &Peer,
    ) -> Result<Arc<PeerManager>, PeerManagerError> {
        std::fs::create_dir_all(&data_path)?;
        let peer_database_name = PathBuf::from(data_path.as_ref())
            .join(random_name())
            .with_extension("db");
        let database_url = DbConnectionUrl::File(peer_database_name);
        let db_connection = DbConnection::connect_and_migrate(&database_url, MIGRATIONS, Some(5))?;
        let peers_db = PeerDatabaseSql::new(db_connection, this_peer)?;
        Ok(Arc::new(PeerManager::new(peers_db)?))
    }
}

pub fn random_name() -> String {
    let mut rng = rand::thread_rng();
    iter::repeat(())
        .map(|_| rng.sample(Alphanumeric) as char)
        .take(12)
        .collect::<String>()
}
