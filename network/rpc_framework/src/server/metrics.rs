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

use libp2p::{PeerId, StreamProtocol};
use once_cell::sync::Lazy;
use tari_metrics::{Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec};

use crate::{RpcServerError, RpcStatusCode};

pub fn num_sessions(peer_id: &PeerId, protocol: &StreamProtocol) -> IntGauge {
    static METER: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec(
            "comms::rpc::server::num_sessions",
            "The number of active server sessions per peer per protocol",
            &["peer_id", "protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer_id.to_string().as_str(), protocol.as_ref()])
}

pub fn handshake_error_counter(peer_id: &PeerId, protocol: &StreamProtocol) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::server::handshake_error_count",
            "The number of handshake errors per peer per protocol",
            &["peer_id", "protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer_id.to_string().as_str(), protocol.as_ref()])
}

pub fn error_counter(peer_id: &PeerId, protocol: &StreamProtocol, err: &RpcServerError) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::server::error_count",
            "The number of RPC errors per peer per protocol",
            &["peer_id", "protocol", "error"],
        )
        .unwrap()
    });

    METER.with_label_values(&[
        peer_id.to_string().as_str(),
        protocol.as_ref(),
        err.to_debug_string().as_str(),
    ])
}

pub fn status_error_counter(peer_id: &PeerId, protocol: &StreamProtocol, status_code: RpcStatusCode) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::server::status_error_count",
            "The number of RPC errors by status code per peer per protocol",
            &["peer_id", "protocol", "status"],
        )
        .unwrap()
    });

    METER.with_label_values(&[
        peer_id.to_string().as_str(),
        protocol.as_ref(),
        status_code.to_debug_string().as_str(),
    ])
}

pub fn inbound_requests_bytes(peer_id: &PeerId, protocol: &StreamProtocol) -> Histogram {
    static METER: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::server::inbound_request_bytes",
            "Avg. request bytes per peer per protocol",
            &["peer_id", "protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer_id.to_string().as_str(), protocol.as_ref()])
}

pub fn outbound_response_bytes(peer_id: &PeerId, protocol: &StreamProtocol) -> Histogram {
    static METER: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::server::outbound_response_bytes",
            "Avg. response bytes per peer per protocol",
            &["peer_id", "protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[peer_id.to_string().as_str(), protocol.as_ref()])
}
