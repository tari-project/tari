// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::future::Future;

use libp2p::PeerId;

use crate::{
    pool::{RpcClientPool, RpcPoolClient},
    NamedProtocolService,
    RpcClient,
    RpcClientBuilder,
};

pub trait RpcConnector: Send {
    type Error: std::error::Error;

    fn connect_rpc<T>(&mut self, peer_id: PeerId) -> impl Future<Output = Result<T, Self::Error>> + Send
    where T: From<RpcClient> + NamedProtocolService + Send {
        async move { self.connect_rpc_using_builder(RpcClientBuilder::new(peer_id)).await }
    }

    fn connect_rpc_using_builder<T>(
        &mut self,
        builder: RpcClientBuilder<T>,
    ) -> impl Future<Output = Result<T, Self::Error>> + Send
    where
        T: From<RpcClient> + NamedProtocolService + Send;

    fn create_rpc_client_pool<T>(
        &self,
        max_sessions: usize,
        client_config: RpcClientBuilder<T>,
    ) -> RpcClientPool<Self, T>
    where
        Self: Clone,
        T: RpcPoolClient + From<RpcClient> + NamedProtocolService + Clone + Send,
    {
        RpcClientPool::new(self.clone(), max_sessions, client_config)
    }
}
