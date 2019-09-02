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

use std::{thread, time::Duration};
use tari_comms::connection::{
    connection::Connection,
    monitor::{ConnectionMonitor, SocketEventType},
    types::Direction,
    zmq::{ZmqContext, ZmqEndpoint},
    InprocAddress,
    NetAddress,
};

use crate::support::factories::{self, TestFactory};

#[test]
fn recv_socket_events() {
    let ctx = ZmqContext::new();
    let monitor_addr = InprocAddress::random();
    let address = factories::net_address::create().use_os_port().build().unwrap();

    let monitor = ConnectionMonitor::connect(&ctx, &monitor_addr).unwrap();

    let conn_in = Connection::new(&ctx, Direction::Inbound)
        .set_monitor_addr(monitor_addr.clone())
        .establish(&address)
        .unwrap();
    let connected_address = NetAddress::from(conn_in.get_connected_address().clone().unwrap());

    {
        // Connect and disconnect
        let conn_out = Connection::new(&ctx, Direction::Outbound)
            .establish(&connected_address)
            .unwrap();
        conn_out.send(&["test".as_bytes()]).unwrap();

        let _ = conn_in.receive(1000).unwrap();
    }

    thread::sleep(Duration::from_millis(10));
    // Collect events
    let mut events = vec![];
    while let Ok(event) = monitor.read(10) {
        events.push(event);
    }

    let event = events.iter().find(|e| e.event_type == SocketEventType::Listening);
    assert!(event.is_some(), "Expected to find event Listening");
    let event = event.unwrap();
    assert_eq!(event.address, connected_address.to_zmq_endpoint());

    let event = events.iter().find(|e| e.event_type == SocketEventType::Accepted);
    assert!(event.is_some(), "Expected to find event Accepted");
    let event = event.unwrap();
    assert_eq!(event.address, connected_address.to_zmq_endpoint());

    let event = events.iter().find(|e| e.event_type == SocketEventType::Disconnected);
    assert!(event.is_some(), "Expected to find event Disconnected");
    let event = event.unwrap();
    assert_eq!(event.address, connected_address.to_zmq_endpoint());
}
