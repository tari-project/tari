// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_network::Connection;

use crate::tari_rpc as grpc;

impl From<Connection> for grpc::ConnectedPeer {
    fn from(conn: Connection) -> Self {
        let public_key = conn
            .public_key
            .as_ref()
            .map(|pk| pk.encode_protobuf())
            .unwrap_or_default();
        let peer_id = conn.peer_id.to_bytes();
        Self {
            public_key,
            peer_id,
            addresses: vec![conn.address().to_vec()],
            user_agent: conn.user_agent.map(|s| (*s).clone()).unwrap_or_default(),
        }
    }
}
