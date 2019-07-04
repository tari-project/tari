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

use std::sync::mpsc::sync_channel;
use tari_comms::{
    connection::{Connection, Direction, InprocAddress, ZmqContext},
    outbound_message_service::{
        outbound_message_pool::{MessageRetryService, OutboundMessagePoolConfig},
        OutboundMessage,
    },
    peer_manager::NodeId,
};
use tari_utilities::message_format::MessageFormat;

/// Tests that the MessageRetryService forwards failed messages after a time.
///
/// 1. Create inbound and outbound connections
/// 2. Sends a message to the MessageRetryService
/// 3. Receives it and checks the attempt count
/// 4. Resends it to the MessageRetryService, and checks the attempt count again
/// 5. Shuts the MessageRetryService down
#[test]
fn message_retries() {
    let context = ZmqContext::new();
    let in_address = InprocAddress::random();
    let out_address = InprocAddress::random();
    let (shutdown_tx, shutdown_rx) = sync_channel(1);

    let sender_connection = Connection::new(&context, Direction::Outbound)
        .establish(&in_address)
        .unwrap();

    let receive_connection = Connection::new(&context, Direction::Inbound)
        .establish(&out_address)
        .unwrap();

    let join_handle = MessageRetryService::start(
        context,
        OutboundMessagePoolConfig::default(),
        in_address,
        out_address,
        shutdown_rx,
    );

    let mut msg = OutboundMessage::new(NodeId::new(), vec![vec![]]);

    sender_connection.send(&[msg.to_binary().unwrap()]).unwrap();

    let frames = receive_connection.receive(2000).unwrap();
    msg = OutboundMessage::from_binary(&frames[1]).unwrap();

    assert_eq!(msg.num_attempts(), 1);
    assert_eq!(msg.scheduled_duration().num_seconds(), 0);

    sender_connection.send(&[msg.to_binary().unwrap()]).unwrap();

    let frames = receive_connection.receive(3000).unwrap();
    msg = OutboundMessage::from_binary(&frames[1]).unwrap();

    assert_eq!(msg.num_attempts(), 2);
    assert!(msg.scheduled_duration().num_seconds() <= 0);

    shutdown_tx.send(()).unwrap();
    join_handle.join().unwrap().unwrap();
}
