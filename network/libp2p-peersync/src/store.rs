//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use libp2p::{
    futures,
    futures::{stream::BoxStream, Stream, StreamExt},
    Multiaddr,
    PeerId,
};

use crate::peer_record::SignedPeerRecord;
#[async_trait]
pub trait PeerStore: Clone + Send + Sync + 'static {
    type Error: std::error::Error;
    type Stream: Stream<Item = Result<SignedPeerRecord, Self::Error>> + Unpin + Send;

    async fn get(&self, peer_id: &PeerId) -> Result<Option<SignedPeerRecord>, Self::Error>;
    async fn put(&self, peer: SignedPeerRecord) -> Result<(), Self::Error>;
    async fn put_if_newer(&self, peer: SignedPeerRecord) -> Result<(), Self::Error> {
        if let Some(existing) = self.get(&peer.to_peer_id()).await? {
            if existing.updated_at >= peer.updated_at {
                return Ok(());
            }
        }
        self.put(peer).await
    }
    async fn put_address(&self, peer_id: &PeerId, address: Multiaddr) -> Result<bool, Self::Error>;
    async fn remove(&self, peer_id: &PeerId) -> Result<Option<SignedPeerRecord>, Self::Error>;

    async fn difference<'a, I: IntoIterator<Item = &'a PeerId> + Send>(
        &self,
        peers: I,
    ) -> Result<HashSet<PeerId>, Self::Error>;
    fn stream(&self) -> Self::Stream;
}

#[derive(Debug, Clone, Default)]
pub struct MemoryPeerStore {
    peers: Arc<RwLock<HashMap<PeerId, SignedPeerRecord>>>,
}

impl MemoryPeerStore {
    pub fn new() -> Self {
        Self {
            peers: Default::default(),
        }
    }
}

#[async_trait]
impl PeerStore for MemoryPeerStore {
    type Error = Infallible;
    type Stream = BoxStream<'static, Result<SignedPeerRecord, Self::Error>>;

    async fn get(&self, peer_id: &PeerId) -> Result<Option<SignedPeerRecord>, Self::Error> {
        Ok(self.peers.read().unwrap().get(peer_id).cloned())
    }

    async fn put(&self, peer: SignedPeerRecord) -> Result<(), Self::Error> {
        tracing::debug!("STORE: put: {:?}", peer);
        self.peers.write().unwrap().insert(peer.to_peer_id(), peer);
        Ok(())
    }

    async fn put_address(&self, peer_id: &PeerId, address: Multiaddr) -> Result<bool, Self::Error> {
        match self.get(peer_id).await? {
            Some(mut peer) => {
                peer.addresses.push(address);
                self.put(peer).await?;
                Ok(true)
            },
            None => Ok(false),
        }
    }

    async fn remove(&self, peer_id: &PeerId) -> Result<Option<SignedPeerRecord>, Self::Error> {
        Ok(self.peers.write().unwrap().remove(peer_id))
    }

    async fn difference<'a, I>(&self, peers: I) -> Result<HashSet<PeerId>, Self::Error>
    where I: IntoIterator<Item = &'a PeerId> + Send {
        let peers = peers.into_iter().copied().collect::<HashSet<_>>();
        Ok(peers
            .difference(&self.peers.read().unwrap().keys().copied().collect::<HashSet<_>>())
            .copied()
            .collect())
    }

    fn stream(&self) -> Self::Stream {
        futures::stream::iter(self.peers.read().unwrap().values().cloned().map(Ok).collect::<Vec<_>>()).boxed()
    }
}
