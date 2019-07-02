//  Copyright 2019 The Tari Project
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

use crate::support::comms_and_services::setup_text_message_service;

use crate::support::utils::assert_change;
use std::path::PathBuf;
use tari_comms::peer_manager::NodeIdentity;
use tari_storage::lmdb_store::{LMDBBuilder, LMDBError, LMDBStore};

fn get_path(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(name);
    path.to_str().unwrap().to_string()
}

fn init_datastore(name: &str) -> Result<LMDBStore, LMDBError> {
    let path = get_path(name);
    let _ = std::fs::create_dir(&path).unwrap_or_default();
    LMDBBuilder::new()
        .set_path(&path)
        .set_environment_size(10)
        .set_max_number_of_databases(1)
        .add_database(name, lmdb_zero::db::CREATE)
        .build()
}

fn clean_up_datastore(name: &str) {
    std::fs::remove_dir_all(get_path(name)).unwrap();
}

#[test]
fn test_text_message_service() {
    let _ = simple_logger::init();

    let mut rng = rand::OsRng::new().unwrap();

    let node_1_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31523".parse().unwrap()).unwrap();
    let node_2_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31545".parse().unwrap()).unwrap();
    let node_3_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31546".parse().unwrap()).unwrap();

    let node_1_database_name = "node_1_test_text_message_service"; // Note: every test should have unique database
    let node_1_datastore = init_datastore(node_1_database_name).unwrap();
    let node_1_peer_database = node_1_datastore.get_handle(node_1_database_name).unwrap();
    let node_2_database_name = "node_2_test_text_message_service"; // Note: every test should have unique database
    let node_2_datastore = init_datastore(node_2_database_name).unwrap();
    let node_2_peer_database = node_2_datastore.get_handle(node_2_database_name).unwrap();
    let node_3_database_name = "node_1_test_text_message_service"; // Note: every test should have unique database
    let node_3_datastore = init_datastore(node_1_database_name).unwrap();
    let node_3_peer_database = node_1_datastore.get_handle(node_1_database_name).unwrap();

    let (node_1_services, node_1_tms) = setup_text_message_service(
        node_1_identity.clone(),
        vec![node_2_identity.clone(), node_3_identity.clone()],
        node_1_peer_database,
    );
    let (node_2_services, node_2_tms) = setup_text_message_service(
        node_2_identity.clone(),
        vec![node_1_identity.clone()],
        node_2_peer_database,
    );
    let (node_3_services, _node_3_tms) = setup_text_message_service(
        node_3_identity.clone(),
        vec![node_1_identity.clone()],
        node_3_peer_database,
    );

    let (node_1_services, node_1_tms) = setup_text_message_service(
        node_1_identity.clone(),
        vec![node_2_identity.clone()],
        node_1_peer_database,
    );
    let (node_2_services, node_2_tms) = setup_text_message_service(
        node_2_identity.clone(),
        vec![node_1_identity.clone()],
        node_2_peer_database,
    );

    node_1_tms
        .send_text_message(node_2_identity.identity.public_key.clone(), "Say Hello".to_string())
        .unwrap();
    node_1_tms
        .send_text_message(node_3_identity.identity.public_key.clone(), "Say Hello".to_string())
        .unwrap();
    node_2_tms
        .send_text_message(node_1_identity.identity.public_key.clone(), "hello?".to_string())
        .unwrap();
    node_1_tms
        .send_text_message(
            node_2_identity.identity.public_key.clone(),
            "to my little friend!".to_string(),
        )
        .unwrap();

    for i in 0..3 {
        node_1_tms
            .send_text_message(
                node_2_identity.identity.public_key.clone(),
                format!("Message {}", i).to_string(),
            )
            .unwrap();
    }
    for i in 0..3 {
        node_2_tms
            .send_text_message(
                node_1_identity.identity.public_key.clone(),
                format!("Message {}", i).to_string(),
            )
            .unwrap();
    }

    assert_change(
        || {
            let msgs = node_1_tms.get_text_messages().unwrap();
            (msgs.sent_messages.len(), msgs.received_messages.len())
        },
        (6, 4),
        100,
    );

    assert_change(
        || {
            let msgs = node_2_tms.get_text_messages().unwrap();
            (msgs.sent_messages.len(), msgs.received_messages.len())
        },
        (4, 5),
        100,
    );

    let msgs = node_1_tms
        .get_text_messages_by_pub_key(node_2_identity.identity.public_key)
        .unwrap();

    assert_eq!(msgs.sent_messages.len(), 5);

    node_1_services.shutdown().unwrap();
    node_2_services.shutdown().unwrap();
    node_3_services.shutdown().unwrap();

    clean_up_datastore(node_1_database_name);
    clean_up_datastore(node_2_database_name);
    clean_up_datastore(node_3_database_name);
}
