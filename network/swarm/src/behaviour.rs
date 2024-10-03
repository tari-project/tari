//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::Hasher,
};

use libp2p::{
    autonat,
    connection_limits,
    connection_limits::ConnectionLimits,
    dcutr,
    gossipsub,
    identify,
    identity::Keypair,
    mdns,
    noise,
    ping,
    relay,
    swarm::{behaviour::toggle::Toggle, NetworkBehaviour},
    tcp,
    yamux,
    StreamProtocol,
    Swarm,
    SwarmBuilder,
};
use libp2p_messaging as messaging;
use libp2p_peersync as peer_sync;
use libp2p_peersync::store::MemoryPeerStore;
use libp2p_substream as substream;

use crate::{
    config::{Config, RelayCircuitLimits, RelayReservationLimits},
    error::TariSwarmError,
};

#[derive(NetworkBehaviour)]
pub struct TariNodeBehaviour<TCodec>
where TCodec: messaging::Codec + Send + Clone + 'static
{
    pub ping: ping::Behaviour,
    pub dcutr: dcutr::Behaviour,
    pub connection_limits: connection_limits::Behaviour,

    pub relay: Toggle<relay::Behaviour>,
    pub relay_client: relay::client::Behaviour,
    pub autonat: autonat::Behaviour,

    pub identify: identify::Behaviour,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
    pub peer_sync: peer_sync::Behaviour<MemoryPeerStore>,

    pub substream: substream::Behaviour,
    pub messaging: Toggle<messaging::Behaviour<TCodec>>,
    pub gossipsub: gossipsub::Behaviour,
}

/// Returns true if the given Multiaddr is supported by the Tari swarm, otherwise false.
/// NOTE: this function only currently returns false for onion addresses.
pub fn is_supported_multiaddr(addr: &libp2p::Multiaddr) -> bool {
    !addr.iter().any(|p| {
        matches!(
            p,
            libp2p::core::multiaddr::Protocol::Onion(_, _) | libp2p::core::multiaddr::Protocol::Onion3(_)
        )
    })
}

pub fn create_swarm<TCodec>(
    identity: Keypair,
    supported_protocols: HashSet<StreamProtocol>,
    config: Config,
) -> Result<Swarm<TariNodeBehaviour<TCodec>>, TariSwarmError>
where
    TCodec: messaging::Codec + Clone + Send + 'static,
{
    let swarm = SwarmBuilder::with_existing_identity(identity)
        .with_tokio()
        .with_tcp(
            tcp::Config::new().nodelay(true).port_reuse(true),
            noise_config,
            yamux::Config::default,
        )?
        .with_quic()
        .with_relay_client(noise_config, yamux::Config::default)?
        .with_behaviour(|keypair, relay_client| {
            let local_peer_id = keypair.public().to_peer_id();

            // Gossipsub
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .validate_messages()
                .message_id_fn(get_message_id) // content-address messages. No two messages of the same content will be propagated.
                .build()
                .unwrap();

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(keypair.clone()),
                gossipsub_config,
            )
            .unwrap();

            // Ping
            let ping = ping::Behaviour::new(config.ping);

            // Dcutr
            let dcutr = dcutr::Behaviour::new(local_peer_id);

            // Relay
            let maybe_relay = if config.enable_relay {
                Some(relay::Behaviour::new(
                    local_peer_id,
                    create_relay_config(&config.relay_circuit_limits, &config.relay_reservation_limits),
                ))
            } else {
                None
            };

            // Identify
            let identify = identify::Behaviour::new(
                identify::Config::new(config.protocol_version.to_string(), keypair.public())
                    .with_interval(config.identify_interval)
                    .with_agent_version(config.user_agent),
            );

            // Messaging
            let messaging = if config.enable_messaging {
                Some(messaging::Behaviour::new(
                    StreamProtocol::try_from_owned(config.messaging_protocol)?,
                    messaging::Config::default(),
                ))
            } else {
                None
            };

            // Substreams
            let substream = substream::Behaviour::new(supported_protocols, substream::Config::default());

            // Connection limits
            let connection_limits = connection_limits::Behaviour::new(
                ConnectionLimits::default().with_max_established_per_peer(config.max_connections_per_peer),
            );

            // mDNS
            let maybe_mdns = if config.enable_mdns {
                Some(mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?)
            } else {
                None
            };

            // autonat
            let autonat = autonat::Behaviour::new(local_peer_id, autonat::Config::default());

            // Peer sync
            let peer_sync =
                peer_sync::Behaviour::new(keypair.clone(), MemoryPeerStore::new(), peer_sync::Config::default());

            Ok(TariNodeBehaviour {
                ping,
                dcutr,
                identify,
                relay: Toggle::from(maybe_relay),
                relay_client,
                autonat,
                gossipsub,
                substream,
                messaging: Toggle::from(messaging),
                connection_limits,
                mdns: Toggle::from(maybe_mdns),
                peer_sync,
            })
        })
        .map_err(|e| TariSwarmError::BehaviourError(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(config.idle_connection_timeout))
        .build();

    Ok(swarm)
}

fn create_relay_config(circuit: &RelayCircuitLimits, reservations: &RelayReservationLimits) -> relay::Config {
    let mut config = relay::Config {
        reservation_rate_limiters: vec![],
        circuit_src_rate_limiters: vec![],
        ..Default::default()
    };

    config.max_circuits = circuit.max_limit;
    config.max_circuits_per_peer = circuit.max_per_peer;
    config.max_circuit_duration = circuit.max_duration;
    config.max_circuit_bytes = circuit.max_byte_limit;
    if let Some(ref limits) = circuit.per_peer {
        config = config.circuit_src_per_peer(limits.limit, limits.interval);
    }

    if let Some(ref limits) = circuit.per_ip {
        config = config.circuit_src_per_ip(limits.limit, limits.interval);
    }

    config.max_reservations = reservations.max_limit;
    config.max_reservations_per_peer = reservations.max_per_peer;
    config.reservation_duration = reservations.max_duration;
    if let Some(ref limits) = reservations.per_peer {
        config = config.reservation_rate_per_peer(limits.limit, limits.interval);
    }

    if let Some(ref limits) = reservations.per_ip {
        config = config.reservation_rate_per_ip(limits.limit, limits.interval);
    }

    config
}

/// Generates a hash of contents of the message
fn get_message_id(message: &gossipsub::Message) -> gossipsub::MessageId {
    let mut hasher = DefaultHasher::new();
    hasher.write(&message.data);
    hasher.write(message.topic.as_str().as_bytes());
    gossipsub::MessageId::from(hasher.finish().to_be_bytes())
}

fn noise_config(keypair: &Keypair) -> Result<noise::Config, noise::Error> {
    Ok(noise::Config::new(keypair)?.with_prologue(noise_prologue()))
}

fn noise_prologue() -> Vec<u8> {
    const PROLOGUE: &str = "tari-digital-asset-network";
    PROLOGUE.as_bytes().to_vec()
}
