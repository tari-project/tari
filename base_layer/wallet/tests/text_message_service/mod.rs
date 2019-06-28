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
use tari_comms::peer_manager::NodeIdentity;

#[test]
fn test_text_message_service() {
    let _ = simple_logger::init();

    let mut rng = rand::OsRng::new().unwrap();

    let node_1_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31523".parse().unwrap()).unwrap();
    let node_2_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31545".parse().unwrap()).unwrap();

    let (node_1_services, node_1_tms) =
        setup_text_message_service(node_1_identity.clone(), vec![node_2_identity.clone()]);
    let (node_2_services, node_2_tms) =
        setup_text_message_service(node_2_identity.clone(), vec![node_1_identity.clone()]);

    node_1_tms
        .send_text_message(node_2_identity.identity.public_key.clone(), "Say Hello,".to_string())
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
        (5, 4),
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

    node_1_services.shutdown().unwrap();
    node_2_services.shutdown().unwrap();
}
