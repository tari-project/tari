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

use std::sync::mpsc::sync_channel;
use tari_comms::{
    connection::{Context, NetAddresses},
    connection_manager::{ConnectionDirection, ConnectionManagerError},
};

#[test]
fn get_active_connection() {
    let context = Context::new();
    let node_id = makers::make_node_id();
    let (establisher, secret_key, _) = makers::make_live_peer_connections(&context);

    establisher
        .establish_connection(ConnectionDirection::Inbound {
            node_id: node_id.clone(),
            secret_key,
        })
        .unwrap();

    let conn = establisher.get_active_connection(&node_id).unwrap();
    let result = conn.wait_connected_or_failure(Duration::from_millis(2000));
    // The connection is listening on an open port so will succeed
    result.unwrap();
    conn.shutdown().unwrap();
    conn.wait_disconnected(Duration::from_millis(2000)).unwrap();

    assert!(establisher.get_active_connection(&node_id).is_none());
}

#[test]
fn drop_connection() {
    let context = Context::new();
    let node_id = makers::make_node_id();
    let (establisher, _secret_key, public_key) = makers::make_live_peer_connections(&context);

    let net_addresses: NetAddresses = makers::make_net_addresses(3);

    establisher
        .establish_connection(ConnectionDirection::Outbound {
            node_id: node_id.clone(),
            net_addresses,
            server_public_key: public_key,
        })
        .unwrap();

    let conn = establisher.get_connection(&node_id).unwrap();
    assert!(conn.wait_connected_or_failure(Duration::from_millis(2000)).is_err());

    assert!(establisher.drop_connection(&node_id).is_ok());
    assert!(establisher.get_connection(&node_id).is_none());
}

#[test]
fn shutdown_wait() {
    let (tx, rx) = sync_channel(1);
    let handle = thread::spawn(move || -> Result<(), ConnectionManagerError> {
        let context = Context::new();
        let node_id = makers::make_node_id();
        let (establisher, secret_key, _public_key) = makers::make_live_peer_connections(&context);

        establisher.establish_connection(ConnectionDirection::Inbound {
            node_id: node_id.clone(),
            secret_key: secret_key.clone(),
        })?;

        let conn1 = establisher
            .get_connection(&node_id)
            .ok_or(ConnectionManagerError::PeerConnectionNotFound)?;

        let node_id = makers::make_node_id();
        establisher.establish_connection(ConnectionDirection::Inbound {
            node_id: node_id.clone(),
            secret_key: secret_key.clone(),
        })?;

        let conn2 = establisher
            .get_connection(&node_id)
            .ok_or(ConnectionManagerError::PeerConnectionNotFound)?;

        conn1.wait_connected_or_failure(Duration::from_millis(2000))?;

        establisher.shutdown_wait()?;

        assert!(conn1.is_disconnected() && conn2.is_disconnected());
        tx.send(()).unwrap();
        Ok(())
    });

    // Wait for this task to finish or fail
    let wait_result = rx.recv_timeout(Duration::from_millis(2000));

    // Emit the error if we failed
    handle.join().unwrap().unwrap();
    // If the thread panicked
    assert!(!wait_result.is_err(), "Task thread panicked");
}

mod makers {
    use std::iter::repeat_with;

    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    use tari_comms::{
        connection::{
            curve_keypair::{CurvePublicKey, CurveSecretKey},
            net_address::NetAddressWithStats,
            Context,
            CurveEncryption,
            InprocAddress,
            NetAddresses,
        },
        connection_manager::{LivePeerConnections, PeerConnectionConfig},
        peer_manager::NodeId,
    };

    use std::{str::FromStr, time::Duration};
    use tari_comms::connection::NetAddress;

    pub(super) fn make_node_id() -> NodeId {
        let (_secret_key, public_key) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
        NodeId::from_key(&public_key).unwrap()
    }

    pub(super) fn make_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            socks_proxy_address: None,
            consumer_address,
            max_connect_retries: 5,
            max_message_size: 512 * 1024,
            establish_timeout: Duration::from_millis(2000),
        }
    }

    pub(super) fn make_establisher_with_config(
        context: &Context,
        config: PeerConnectionConfig,
    ) -> (LivePeerConnections, CurveSecretKey, CurvePublicKey)
    {
        let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();
        let establisher = LivePeerConnections::new(context.clone(), config);
        (establisher, secret_key, public_key)
    }

    pub(super) fn make_live_peer_connections(
        context: &Context,
    ) -> (LivePeerConnections, CurveSecretKey, CurvePublicKey) {
        let consumer_address = InprocAddress::random();
        make_establisher_with_config(context, make_config(consumer_address))
    }

    pub(super) fn make_net_addresses(count: usize) -> NetAddresses {
        let address_maker = || NetAddress::from_str("127.0.0.1:0").unwrap().into();
        repeat_with(address_maker)
            .take(count)
            .collect::<Vec<NetAddressWithStats>>()
            .into()
    }
}
