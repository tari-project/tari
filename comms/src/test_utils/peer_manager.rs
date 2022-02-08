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

use std::sync::Arc;

use crate::PeerManager;

#[cfg(test)]
pub fn build_peer_manager() -> Arc<PeerManager> {
    Arc::new(PeerManager::new(tari_storage::HashmapDatabase::new(), None).unwrap())
}

#[cfg(not(test))]
pub use not_test::build_peer_manager;

#[cfg(not(test))]
mod not_test {
    use std::{iter, path::Path};

    use rand::{distributions::Alphanumeric, Rng};
    use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};

    use super::*;

    pub fn build_peer_manager<P: AsRef<Path>>(data_path: P) -> Arc<PeerManager> {
        let peer_database_name = {
            let mut rng = rand::thread_rng();
            iter::repeat(())
                .map(|_| rng.sample(Alphanumeric) as char)
                .take(8)
                .collect::<String>()
        };
        std::fs::create_dir_all(&data_path).unwrap();
        let datastore = LMDBBuilder::new()
            .set_path(data_path)
            .set_env_config(Default::default())
            .set_max_number_of_databases(1)
            .add_database(&peer_database_name, lmdb_zero::db::CREATE)
            .build()
            .unwrap();
        let peer_database = datastore.get_handle(&peer_database_name).unwrap();
        Arc::new(PeerManager::new(LMDBWrapper::new(Arc::new(peer_database)), None).unwrap())
    }
}
