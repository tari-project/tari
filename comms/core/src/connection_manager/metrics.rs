//  Copyright 2021, The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use once_cell::sync::Lazy;
use taiji_metrics::{IntCounter, IntCounterVec, IntGauge, IntGaugeVec};

use crate::{connection_manager::ConnectionDirection, peer_manager::NodeId, protocol::ProtocolId};

pub fn pending_connections(peer: Option<&NodeId>, direction: ConnectionDirection) -> IntGauge {
    static METER: Lazy<IntGaugeVec> = Lazy::new(|| {
        taiji_metrics::register_int_gauge_vec(
            "comms::connections::pending",
            "Number of active connections by direction",
            &["peer_id", "direction"],
        )
        .unwrap()
    });

    METER.with_label_values(&[
        peer.map(ToString::to_string)
            .unwrap_or_else(|| "unknown".to_string())
            .as_str(),
        direction.as_str(),
    ])
}

pub fn successful_connections(peer: &NodeId, direction: ConnectionDirection) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        taiji_metrics::register_int_counter_vec(
            "comms::connections::success",
            "Number of active connections by direction",
            &["peer_id", "direction"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer.to_string().as_str(), direction.as_str()])
}

pub fn failed_connections(peer: &NodeId, direction: ConnectionDirection) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        taiji_metrics::register_int_counter_vec(
            "comms::connections::failed",
            "Number of active connections by direction",
            &["peer_id", "direction"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer.to_string().as_str(), direction.as_str()])
}

pub fn inbound_substream_counter(peer: &NodeId, protocol: &ProtocolId) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        taiji_metrics::register_int_counter_vec(
            "comms::connections::inbound_substream_request_count",
            "Number of substream requests",
            &["peer_id", "protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}
