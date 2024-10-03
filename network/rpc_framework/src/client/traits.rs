// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use libp2p::PeerId;

use crate::{NamedProtocolService, RpcClient, RpcClientBuilder};

pub trait RpcConnector {
    type Error;

    async fn is_connected(&self, peer_id: &PeerId) -> Result<bool, Self::Error>;

    async fn connect_rpc<T>(&mut self, peer_id: PeerId) -> Result<T, Self::Error>
    where T: From<RpcClient> + NamedProtocolService {
        self.connect_rpc_using_builder(RpcClientBuilder::new(peer_id)).await
    }

    async fn connect_rpc_using_builder<T>(&mut self, builder: RpcClientBuilder<T>) -> Result<T, Self::Error>
    where T: From<RpcClient> + NamedProtocolService;
}
