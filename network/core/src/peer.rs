//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{Multiaddr, PeerId, StreamProtocol};

pub struct PeerInfo {
    pub peer_id: PeerId,
    pub protocol_version: String,
    pub agent_version: String,
    pub listen_addrs: Vec<Multiaddr>,
    pub protocols: Vec<StreamProtocol>,
    // pub observed_addr: Multiaddr,
}

impl std::fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        print_key_value("PeerId", &self.peer_id, f)?;
        print_key_value("Protocol version", &self.protocol_version, f)?;
        print_key_value("Agent version", &self.agent_version, f)?;
        // print_key_value("Observed address", &self.observed_addr, f)?;
        if self.listen_addrs.is_empty() {
            writeln!(f, "No listener addresses")?;
        } else {
            print_key("Listen addresses", f)?;
            for addr in &self.listen_addrs {
                writeln!(f, "\t- {addr:?}")?;
            }
        }
        if !self.protocols.is_empty() {
            print_key("Protocols", f)?;
            for protocol in &self.protocols {
                writeln!(f, "\t- {protocol}")?;
            }
        }

        Ok(())
    }
}
fn print_key_value<V: std::fmt::Debug>(k: &str, v: &V, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{k}: {v:?}")
}
fn print_key(k: &str, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "{k}:")
}
