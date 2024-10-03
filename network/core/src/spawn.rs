//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::collections::HashSet;

use anyhow::anyhow;
use libp2p::{identity::Keypair, Multiaddr, PeerId};
use tari_shutdown::ShutdownSignal;
use tari_swarm::{is_supported_multiaddr, messaging, messaging::prost::ProstCodec};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

use crate::{message::MessageSpec, worker::NetworkingWorker, NetworkingHandle};

pub fn spawn<TMsg>(
    identity: Keypair,
    messaging_mode: MessagingMode<TMsg>,
    mut config: crate::Config,
    seed_peers: Vec<(PeerId, Multiaddr)>,
    shutdown_signal: ShutdownSignal,
) -> anyhow::Result<(NetworkingHandle<TMsg>, JoinHandle<anyhow::Result<()>>)>
where
    TMsg: MessageSpec + 'static,
    TMsg::Message: messaging::prost::Message + Default + Clone + 'static,
    TMsg::GossipMessage: messaging::prost::Message + Default + Clone + 'static,
    TMsg: MessageSpec,
{
    for (_, addr) in &seed_peers {
        if !is_supported_multiaddr(addr) {
            return Err(anyhow!("Unsupported seed peer multi-address: {}", addr));
        }
    }

    config.swarm.enable_relay = config.swarm.enable_relay || !config.reachability_mode.is_private();
    config.swarm.enable_messaging = messaging_mode.is_enabled();
    let swarm =
        tari_swarm::create_swarm::<ProstCodec<TMsg::Message>>(identity.clone(), HashSet::new(), config.swarm.clone())?;
    let local_peer_id = *swarm.local_peer_id();
    let (tx, rx) = mpsc::channel(1);
    let (tx_events, _) = broadcast::channel(100);
    let handle = tokio::spawn(
        NetworkingWorker::<TMsg>::new(
            identity,
            rx,
            tx_events.clone(),
            messaging_mode,
            swarm,
            config,
            seed_peers,
            shutdown_signal,
        )
        .run(),
    );
    Ok((NetworkingHandle::new(local_peer_id, tx, tx_events), handle))
}

pub enum MessagingMode<TMsg: MessageSpec> {
    Enabled {
        tx_messages: mpsc::UnboundedSender<(PeerId, TMsg::Message)>,
        tx_gossip_messages: mpsc::UnboundedSender<(PeerId, TMsg::GossipMessage)>,
    },
    Disabled,
}

impl<TMsg: MessageSpec> MessagingMode<TMsg> {
    pub fn is_enabled(&self) -> bool {
        matches!(self, MessagingMode::Enabled { .. })
    }
}

impl<TMsg: MessageSpec> MessagingMode<TMsg> {
    pub fn send_message(
        &self,
        peer_id: PeerId,
        msg: TMsg::Message,
    ) -> Result<(), mpsc::error::SendError<(PeerId, TMsg::Message)>> {
        if let MessagingMode::Enabled { tx_messages, .. } = self {
            tx_messages.send((peer_id, msg))?;
        }
        Ok(())
    }

    pub fn send_gossip_message(
        &self,
        peer_id: PeerId,
        msg: TMsg::GossipMessage,
    ) -> Result<(), mpsc::error::SendError<(PeerId, TMsg::GossipMessage)>> {
        if let MessagingMode::Enabled { tx_gossip_messages, .. } = self {
            tx_gossip_messages.send((peer_id, msg))?;
        }
        Ok(())
    }
}
