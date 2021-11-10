//  Copyright 2021, The Tari Project
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

use crate::{peer_manager::NodeId, protocol::ProtocolId};
use once_cell::sync::Lazy;
use tari_metrics::{Histogram, HistogramVec, IntGauge, IntGaugeVec};

pub fn sessions_counter(node_id: &NodeId, protocol: &ProtocolId) -> IntGauge {
    static GAUGE: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec(
            "comms::rpc::client::num_sessions",
            "The number of active clients per node per protocol",
            &["peer", "protocol"],
        )
        .unwrap()
    });

    GAUGE.with_label_values(&[node_id.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}

pub fn handshake_errors(node_id: &NodeId, protocol: &ProtocolId) -> IntGauge {
    static GAUGE: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec(
            "comms::rpc::client::handshake_errors",
            "The number of handshake errors per node per protocol",
            &["peer", "protocol"],
        )
        .unwrap()
    });

    GAUGE.with_label_values(&[node_id.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}

pub fn request_response_latency(node_id: &NodeId, protocol: &ProtocolId) -> Histogram {
    static GAUGE: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::request_response_latency",
            "A histogram of request to first response latency",
            &["peer", "protocol"],
        )
        .unwrap()
    });

    GAUGE.with_label_values(&[node_id.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}

pub fn outbound_request_bytes(node_id: &NodeId, protocol: &ProtocolId) -> Histogram {
    static GAUGE: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::outbound_request_bytes",
            "Avg. request bytes per node per protocol",
            &["peer", "protocol"],
        )
        .unwrap()
    });

    GAUGE.with_label_values(&[node_id.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}

pub fn inbound_response_bytes(node_id: &NodeId, protocol: &ProtocolId) -> Histogram {
    static GAUGE: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::inbound_response_bytes",
            "Avg. response bytes per peer per protocol",
            &["peer", "protocol"],
        )
        .unwrap()
    });

    GAUGE.with_label_values(&[node_id.to_string().as_str(), String::from_utf8_lossy(protocol).as_ref()])
}
