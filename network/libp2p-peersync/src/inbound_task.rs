//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{collections::HashSet, io};

use libp2p::{
    futures::{SinkExt, StreamExt},
    PeerId,
};

use crate::{handler::FramedInbound, proto, store::PeerStore, Config, Error, Event, SignedPeerRecord};

pub async fn inbound_sync_task<TPeerStore: PeerStore>(
    peer_id: PeerId,
    framed: FramedInbound,
    store: TPeerStore,
    config: Config,
) -> Event {
    tracing::debug!("Starting inbound protocol sync with peer {}", peer_id);
    inbound_sync_task_inner(peer_id, framed, store, config)
        .await
        .unwrap_or_else(Event::Error)
}

async fn inbound_sync_task_inner<TPeerStore: PeerStore>(
    peer_id: PeerId,
    mut framed: FramedInbound,
    store: TPeerStore,
    config: Config,
) -> Result<Event, Error> {
    let msg = framed.next().await.ok_or(Error::InboundStreamEnded)??;

    let mut store_stream = store.stream();

    let orig_want_list_len = msg.want_peer_ids.len();
    if orig_want_list_len > config.max_want_list_len {
        tracing::warn!(
            "Peer {} requested {} peers, but the maximum is {}",
            peer_id,
            orig_want_list_len,
            config.max_want_list_len
        );
        return Err(Error::WantListTooLarge {
            want_list_len: orig_want_list_len,
            max_len: config.max_want_list_len,
        });
    }

    let mut remaining_want_list = msg
        .want_peer_ids
        .into_iter()
        .map(|p| PeerId::from_bytes(&p))
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| Error::InvalidMessage {
            peer_id,
            details: format!("invalid peer id in requested want_list: {e}"),
        })?;

    let event = loop {
        if remaining_want_list.is_empty() {
            break Event::ResponseStreamComplete {
                peer_id,
                peers_sent: orig_want_list_len - remaining_want_list.len(),
                requested: orig_want_list_len,
            };
        }

        let Some(result) = store_stream.next().await else {
            break Event::ResponseStreamComplete {
                peer_id,
                peers_sent: orig_want_list_len - remaining_want_list.len(),
                requested: orig_want_list_len,
            };
        };

        let synced_peer: SignedPeerRecord = result.map_err(|e| Error::StoreError(e.to_string()))?;
        let synced_peer_id = synced_peer.to_peer_id();

        if !remaining_want_list.remove(&synced_peer_id) {
            continue;
        }

        if let Err(e) = framed
            .send(proto::WantPeerResponse {
                peer: Some(synced_peer.into()),
            })
            .await
        {
            let e = io::Error::from(e);
            if e.kind() == io::ErrorKind::UnexpectedEof {
                break Event::InboundStreamInterrupted { peer_id };
            } else {
                break Event::Error(Error::CodecError(e));
            }
        }
    };

    if let Err(err) = framed.close().await {
        tracing::warn!("Error closing inbound sync stream: {}", err);
    }
    Ok(event)
}
