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

use crate::support::{comms_and_services::setup_text_message_service, data::*, utils::assert_change};
use tari_comms::peer_manager::NodeIdentity;
use tari_wallet::text_message_service::Contact;

#[test]
fn test_text_message_service() {
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
    let node_3_database_name = "node_3_test_text_message_service"; // Note: every test should have unique database
    let node_3_datastore = init_datastore(node_3_database_name).unwrap();
    let node_3_peer_database = node_3_datastore.get_handle(node_3_database_name).unwrap();

    let db_name1 = "test_text_message_service1.sqlite3";
    let db_path1 = get_path(Some(db_name1));
    init_sql_database(db_name1);

    let db_name2 = "test_text_message_service2.sqlite3";
    let db_path2 = get_path(Some(db_name2));
    init_sql_database(db_name2);

    let db_name3 = "test_text_message_service3.sqlite3";
    let db_path3 = get_path(Some(db_name3));
    init_sql_database(db_name3);

    let (node_1_services, node_1_tms, _comms_1) = setup_text_message_service(
        node_1_identity.clone(),
        vec![node_2_identity.clone(), node_3_identity.clone()],
        node_1_peer_database,
        db_path1,
    );
    let (node_2_services, node_2_tms, _comms_2) = setup_text_message_service(
        node_2_identity.clone(),
        vec![node_1_identity.clone()],
        node_2_peer_database,
        db_path2,
    );
    let (node_3_services, node_3_tms, _comms_3) = setup_text_message_service(
        node_3_identity.clone(),
        vec![node_1_identity.clone()],
        node_3_peer_database,
        db_path3,
    );

    node_1_tms
        .add_contact(Contact::new(
            "Bob".to_string(),
            node_2_identity.identity.public_key.clone(),
            node_2_identity.control_service_address.clone(),
        ))
        .unwrap();
    node_1_tms
        .add_contact(Contact::new(
            "Carol".to_string(),
            node_3_identity.identity.public_key.clone(),
            node_3_identity.control_service_address.clone(),
        ))
        .unwrap();

    node_2_tms
        .add_contact(Contact::new(
            "Alice".to_string(),
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address.clone(),
        ))
        .unwrap();

    node_3_tms
        .add_contact(Contact::new(
            "Alice".to_string(),
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address.clone(),
        ))
        .unwrap();

    let mut node1_to_node2_sent_messages = vec!["Say Hello".to_string(), "to my little friend!".to_string()];

    node_1_tms
        .send_text_message(
            node_2_identity.identity.public_key.clone(),
            node1_to_node2_sent_messages[0].clone(),
        )
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
            node1_to_node2_sent_messages[1].clone(),
        )
        .unwrap();

    for i in 0..3 {
        node1_to_node2_sent_messages.push(format!("Message {}", i).to_string());
        node_1_tms
            .send_text_message(
                node_2_identity.identity.public_key.clone(),
                node1_to_node2_sent_messages[2 + i].clone(),
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
        50,
    );

    assert_change(
        || {
            let msgs = node_2_tms.get_text_messages().unwrap();
            (msgs.sent_messages.len(), msgs.received_messages.len())
        },
        (4, 5),
        50,
    );

    let node1_msgs = node_1_tms
        .get_text_messages_by_pub_key(node_2_identity.identity.public_key)
        .unwrap();

    assert_eq!(node1_msgs.sent_messages.len(), node1_to_node2_sent_messages.len());
    for i in 0..node1_to_node2_sent_messages.len() {
        assert_eq!(node1_msgs.sent_messages[i].message, node1_to_node2_sent_messages[i]);
    }

    let node2_msgs = node_2_tms
        .get_text_messages_by_pub_key(node_1_identity.identity.public_key)
        .unwrap();

    assert_eq!(node2_msgs.received_messages.len(), node1_to_node2_sent_messages.len());
    for i in 0..node1_to_node2_sent_messages.len() {
        assert_eq!(node2_msgs.received_messages[i].message, node1_to_node2_sent_messages[i]);
    }

    node_1_services.shutdown().unwrap();
    node_2_services.shutdown().unwrap();
    node_3_services.shutdown().unwrap();
    clean_up_datastore(node_1_database_name);
    clean_up_datastore(node_2_database_name);
    clean_up_datastore(node_3_database_name);
    clean_up_sql_database(db_name1);
    clean_up_sql_database(db_name2);
    clean_up_sql_database(db_name3);
}
