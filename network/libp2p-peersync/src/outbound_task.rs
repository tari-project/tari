//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{io, sync::Arc};

use libp2p::{
    futures::{SinkExt, StreamExt},
    PeerId,
};

use crate::{behaviour::WantList, handler::FramedOutbound, proto, store::PeerStore, Error, Event, SignedPeerRecord};

pub async fn outbound_request_want_list_task<TPeerStore: PeerStore>(
    peer_id: PeerId,
    mut framed: FramedOutbound,
    store: TPeerStore,
    want_list: Arc<WantList>,
    local_peer_record: Option<Arc<SignedPeerRecord>>,
) -> Event {
    tracing::debug!("Starting outbound protocol with peer {}", peer_id);
    outbound_request_want_list_task_inner(peer_id, &mut framed, store, want_list, local_peer_record.as_deref())
        .await
        .unwrap_or_else(Event::Error)
}

async fn outbound_request_want_list_task_inner<TPeerStore: PeerStore>(
    peer_id: PeerId,
    framed: &mut FramedOutbound,
    store: TPeerStore,
    want_list: Arc<WantList>,
    local_peer_record: Option<&SignedPeerRecord>,
) -> Result<Event, Error> {
    if let Some(local_peer_record) = local_peer_record {
        tracing::debug!("Sending updated local peer record to peer {}", peer_id);
        let msg = proto::SignedPeerRecord::from(local_peer_record);
        framed.send(msg.into()).await.map_err(|e| Error::CodecError(e.into()))?;
    }

    if want_list.is_empty() {
        tracing::debug!("[peer_id={peer_id}] Empty want list. Protocol complete");
        // Nothing further to do, let the peer know
        framed
            .send(proto::Message {
                payload: proto::mod_Message::OneOfpayload::None,
            })
            .await
            .map_err(|e| Error::CodecError(e.into()))?;
        return Ok(Event::InboundRequestCompleted {
            peer_id,
            peers_sent: 1,
            requested: 0,
        });
    }

    framed
        .send(
            proto::WantPeers {
                want_peer_ids: want_list.iter().map(|p| p.to_bytes()).collect(),
            }
            .into(),
        )
        .await
        .map_err(|e| Error::CodecError(e.into()))?;
    tracing::debug!("Sent want list (size={}) to peer {}", want_list.len(), peer_id);

    let mut new_peers = 0;
    while let Some(msg) = framed.next().await {
        if new_peers + 1 > want_list.len() {
            return Err(Error::InvalidMessage {
                peer_id,
                details: format!("Peer {peer_id} sent us more peers than we requested"),
            });
        }

        match msg {
            Ok(msg) => {
                let Some(peer) = msg.peer else {
                    return Err(Error::InvalidMessage {
                        peer_id,
                        details: "empty message".to_string(),
                    });
                };

                let rec = match SignedPeerRecord::try_from(peer) {
                    Ok(rec) => rec,
                    Err(e) => {
                        return Err(Error::InvalidMessage {
                            peer_id,
                            details: e.to_string(),
                        });
                    },
                };

                if !want_list.contains(&rec.to_peer_id()) {
                    return Err(Error::InvalidMessage {
                        peer_id,
                        details: format!("Peer {peer_id} sent us a peer we didnt request"),
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
                    return Ok(Event::OutboundStreamInterrupted { peer_id });
                } else {
                    return Err(Error::CodecError(e));
                }
            },
        }
    }

    tracing::debug!("Received {} new peers from {}", new_peers, peer_id);

    Ok(Event::PeerBatchReceived {
        from_peer: peer_id,
        new_peers,
    })
}
