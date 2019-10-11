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

use crate::support::{comms_and_services::setup_comms_services, data::*, utils::event_stream_count};
use std::{sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::pubsub_connector,
    services::{comms_outbound::CommsOutboundServiceInitializer, liveness::LivenessInitializer},
};
use tari_service_framework::StackBuilder;
use tari_wallet::text_message_service::{
    handle::{TextMessageEvent, TextMessageHandle},
    model::{Contact, UpdateContact},
    TextMessageServiceInitializer,
};
use tokio::runtime::Runtime;

pub fn setup_text_message_service(
    runtime: &Runtime,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    database_path: String,
) -> (TextMessageHandle, Arc<CommsNode>, Dht)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.executor(), Arc::new(node_identity.clone()), peers, publisher);

    let fut = StackBuilder::new(runtime.executor())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(Arc::clone(&subscription_factory)))
        .add_initializer(TextMessageServiceInitializer::new(
            subscription_factory,
            node_identity.identity.public_key.clone(),
            database_path,
        ))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let tms_api = handles.get_handle::<TextMessageHandle>().unwrap();

    (tms_api, comms, dht)
}

#[test]
fn test_text_message_service() {
    let runtime = Runtime::new().unwrap();

    let mut rng = rand::OsRng::new().unwrap();

    let node_1_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:31523".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let node_2_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:31145".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let node_3_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:31546".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();

    let db_name1 = "test_text_message_service1.sqlite3";
    let db_path1 = get_path(Some(db_name1));
    init_sql_database(db_name1);

    let db_name2 = "test_text_message_service2.sqlite3";
    let db_path2 = get_path(Some(db_name2));
    init_sql_database(db_name2);

    let db_name3 = "test_text_message_service3.sqlite3";
    let db_path3 = get_path(Some(db_name3));
    init_sql_database(db_name3);

    let (mut node_1_tms, _comms_1, _dht_1) = setup_text_message_service(
        &runtime,
        node_1_identity.clone(),
        vec![node_2_identity.clone(), node_3_identity.clone()],
        db_path1,
    );
    let (mut node_2_tms, _comms_2, _dht_2) = setup_text_message_service(
        &runtime,
        node_2_identity.clone(),
        vec![node_1_identity.clone()],
        db_path2,
    );
    let (mut node_3_tms, _comms_3, _dht_3) = setup_text_message_service(
        &runtime,
        node_3_identity.clone(),
        vec![node_1_identity.clone()],
        db_path3,
    );

    runtime
        .block_on(node_1_tms.add_contact(Contact::new(
            "Bob".to_string(),
            node_2_identity.identity.public_key.clone(),
            node_2_identity.control_service_address(),
        )))
        .unwrap();

    runtime
        .block_on(node_1_tms.add_contact(Contact::new(
            "Carol".to_string(),
            node_3_identity.identity.public_key.clone(),
            node_3_identity.control_service_address(),
        )))
        .unwrap();

    runtime
        .block_on(node_2_tms.add_contact(Contact::new(
            "Alice".to_string(),
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address(),
        )))
        .unwrap();

    runtime
        .block_on(node_3_tms.add_contact(Contact::new(
            "Alice".to_string(),
            node_1_identity.identity.public_key.clone(),
            node_1_identity.control_service_address(),
        )))
        .unwrap();
    let mut node1_to_node2_sent_messages = vec!["Say Hello".to_string(), "to my little friend!".to_string()];

    runtime
        .block_on(node_1_tms.send_text_message(
            node_2_identity.identity.public_key.clone(),
            node1_to_node2_sent_messages[0].clone(),
        ))
        .unwrap();

    runtime
        .block_on(node_1_tms.send_text_message(node_3_identity.identity.public_key.clone(), "Say Hello".to_string()))
        .unwrap();

    runtime
        .block_on(node_2_tms.send_text_message(node_1_identity.identity.public_key.clone(), "hello?".to_string()))
        .unwrap();
    runtime
        .block_on(node_1_tms.send_text_message(
            node_2_identity.identity.public_key.clone(),
            node1_to_node2_sent_messages[1].clone(),
        ))
        .unwrap();

    for i in 0..3 {
        node1_to_node2_sent_messages.push(format!("Message {}", i).to_string());
        runtime
            .block_on(node_1_tms.send_text_message(
                node_2_identity.identity.public_key.clone(),
                node1_to_node2_sent_messages[2 + i].clone(),
            ))
            .unwrap();
    }
    for i in 0..3 {
        runtime
            .block_on(node_2_tms.send_text_message(
                node_1_identity.identity.public_key.clone(),
                format!("Message {}", i).to_string(),
            ))
            .unwrap();
    }

    let mut result = runtime
        .block_on(async { event_stream_count(node_1_tms.get_event_stream_fused(), 10, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessage), Some(4));
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessageAck), Some(6));

    let mut result = runtime
        .block_on(async { event_stream_count(node_2_tms.get_event_stream_fused(), 9, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessage), Some(5));
    assert_eq!(result.remove(&TextMessageEvent::ReceivedTextMessageAck), Some(4));

    let node1_msgs = runtime
        .block_on(node_1_tms.get_text_messages_by_pub_key(node_2_identity.identity.public_key))
        .unwrap();

    assert_eq!(node1_msgs.sent_messages.len(), node1_to_node2_sent_messages.len());
    for i in 0..node1_to_node2_sent_messages.len() {
        assert_eq!(node1_msgs.sent_messages[i].message, node1_to_node2_sent_messages[i]);
    }

    let node2_msgs = runtime
        .block_on(node_2_tms.get_text_messages_by_pub_key(node_1_identity.identity.public_key))
        .unwrap();

    assert_eq!(node2_msgs.received_messages.len(), node1_to_node2_sent_messages.len());
    for i in 0..node1_to_node2_sent_messages.len() {
        assert_eq!(node2_msgs.received_messages[i].message, node1_to_node2_sent_messages[i]);
    }

    clean_up_sql_database(db_name1);
    clean_up_sql_database(db_name2);
    clean_up_sql_database(db_name3);
}

#[test]
fn test_text_message_requester_crud() {
    let runtime = Runtime::new().unwrap();
    let mut rng = rand::OsRng::new().unwrap();
    let node_1_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30123".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let node_3_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30546".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();

    // Note: every test should have unique database
    let db_name1 = "test_tms_crud1.sqlite3";
    let db_path1 = get_path(Some(db_name1));
    init_sql_database(db_name1);

    let (mut node_1_tms, _comms_1, _dht_1) = setup_text_message_service(
        &runtime,
        node_1_identity.clone(),
        vec![node_3_identity.clone()],
        db_path1,
    );

    runtime
        .block_on(node_1_tms.set_screen_name("Alice".to_string()))
        .unwrap();

    let sn = runtime.block_on(node_1_tms.get_screen_name()).unwrap();

    assert_eq!(sn, Some("Alice".to_string()));

    runtime
        .block_on(node_1_tms.add_contact(Contact::new(
            "Carol".to_string(),
            node_3_identity.identity.public_key.clone(),
            node_3_identity.control_service_address(),
        )))
        .unwrap();

    assert!(runtime
        .block_on(node_1_tms.add_contact(Contact::new(
            "Carol".to_string(),
            node_3_identity.identity.public_key.clone(),
            node_3_identity.control_service_address(),
        )))
        .is_err());

    let contacts = runtime.block_on(node_1_tms.get_contacts()).unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].screen_name, "Carol".to_string());

    runtime
        .block_on(
            node_1_tms.update_contact(node_3_identity.identity.public_key.clone(), UpdateContact {
                screen_name: Some("Dave".to_string()),
                address: None,
            }),
        )
        .unwrap();

    let contacts = runtime.block_on(node_1_tms.get_contacts()).unwrap();

    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].screen_name, "Dave".to_string());

    runtime
        .block_on(node_1_tms.remove_contact(Contact::new(
            "Dave".to_string(),
            node_3_identity.identity.public_key.clone(),
            node_3_identity.control_service_address(),
        )))
        .unwrap();

    let contacts = runtime.block_on(node_1_tms.get_contacts()).unwrap();

    assert_eq!(contacts.len(), 0);

    clean_up_sql_database(db_name1);
}
