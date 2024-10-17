//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{io, sync::Arc};

use libp2p::{
    futures::{SinkExt, StreamExt},
    PeerId,
};

use crate::{behaviour::WantList, handler::FramedOutbound, proto, store::PeerStore, Error, Event, SignedPeerRecord};

pub async fn outbound_sync_task<TPeerStore: PeerStore>(
    peer_id: PeerId,
    mut framed: FramedOutbound,
    store: TPeerStore,
    want_list: Arc<WantList>,
) -> Event {
    tracing::debug!("Starting outbound protocol sync with peer {}", peer_id);
    outbound_sync_task_inner(peer_id, &mut framed, store, want_list)
        .await
        .unwrap_or_else(Event::Error)
}

async fn outbound_sync_task_inner<TPeerStore: PeerStore>(
    from_peer: PeerId,
    framed: &mut FramedOutbound,
    store: TPeerStore,
    want_list: Arc<WantList>,
) -> Result<Event, Error> {
    {
        framed
            .send(proto::WantPeers {
                want_peer_ids: want_list.iter().map(|p| p.to_bytes()).collect(),
            })
            .await
            .map_err(|e| Error::CodecError(e.into()))?;
        tracing::debug!("Sent want list to peer {}", from_peer);

        let mut new_peers = 0;
        while let Some(msg) = framed.next().await {
            if new_peers + 1 > want_list.len() {
                return Err(Error::InvalidMessage {
                    peer_id: from_peer,
                    details: format!("Peer {from_peer} sent us more peers than we requested"),
                });
            }

            match msg {
                Ok(msg) => {
                    let Some(peer) = msg.peer else {
                        return Err(Error::InvalidMessage {
                            peer_id: from_peer,
                            details: "empty message".to_string(),
                        });
                    };

                    let rec = match SignedPeerRecord::try_from(peer) {
                        Ok(rec) => rec,
                        Err(e) => {
                            return Err(Error::InvalidMessage {
                                peer_id: from_peer,
                                details: e.to_string(),
                            });
                        },
                    };

                    if !want_list.contains(&rec.to_peer_id()) {
                        return Err(Error::InvalidMessage {
                            peer_id: from_peer,
                            details: format!("Peer {from_peer} sent us a peer we didnt request"),
                        });
                    }

                    new_peers += 1;

                    store
                        .put_if_newer(rec)
                        .await
                        .map_err(|err| Error::StoreError(err.to_string()))?;
                },
                Err(e) => {
                    let e = io::Error::from(e);
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(Event::OutboundStreamInterrupted { peer_id: from_peer });
                    } else {
                        return Err(Error::CodecError(e));
                    }
                },
            }
        }

        Ok(Event::PeerBatchReceived { from_peer, new_peers })
    }
}
