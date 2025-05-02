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

use once_cell::sync::Lazy;
use tari_metrics::{Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec};

use crate::protocol::ProtocolId;

pub fn num_sessions(protocol: &ProtocolId) -> IntGauge {
    static METER: Lazy<IntGaugeVec> = Lazy::new(|| {
        tari_metrics::register_int_gauge_vec(
            "comms::rpc::client::num_sessions",
            "The number of active clients per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn handshake_counter(protocol: &ProtocolId) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::client::handshake_count",
            "The number of handshakes per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn handshake_errors(protocol: &ProtocolId) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::client::handshake_errors",
            "The number of handshake errors per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn client_errors(protocol: &ProtocolId) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::client::error_count",
            "The number of client errors per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn client_timeouts(protocol: &ProtocolId) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::rpc::client::error_timeouts",
            "The number of client timeouts per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn request_response_latency(protocol: &ProtocolId) -> Histogram {
    static METER: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::request_response_latency",
            "A histogram of request to first response latency",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn outbound_request_bytes(protocol: &ProtocolId) -> Histogram {
    static METER: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::outbound_request_bytes",
            "Avg. request bytes per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}

pub fn inbound_response_bytes(protocol: &ProtocolId) -> Histogram {
    static METER: Lazy<HistogramVec> = Lazy::new(|| {
        tari_metrics::register_histogram_vec(
            "comms::rpc::client::inbound_response_bytes",
            "Avg. response bytes per protocol",
            &["protocol"],
        )
        .unwrap()
    });

    METER.with_label_values(&[String::from_utf8_lossy(protocol).as_ref()])
}
