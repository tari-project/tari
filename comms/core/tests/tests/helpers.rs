//  Copyright 2022. The Tari Project
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

use rand::rngs::OsRng;
use tari_common_sqlite::connection::DbConnection;
use tari_comms::{
    peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        PeerFeatures,
    },
    types::CommsDatabase,
    CommsBuilder,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_shutdown::ShutdownSignal;

use crate::tests::create_test_peer;

pub fn create_peer_storage() -> CommsDatabase {
    let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
    PeerDatabaseSql::new(db_connection, &create_test_peer()).unwrap()
}

pub fn create_comms(signal: ShutdownSignal) -> UnspawnedCommsNode {
    let node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    CommsBuilder::new()
        .allow_test_addresses()
        .with_listener_address("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .with_node_identity(node_identity)
        .with_peer_storage(create_peer_storage())
        .with_shutdown_signal(signal)
        .build()
        .unwrap()
}
