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
use tari_metrics::{Histogram, HistogramOpts, IntCounter, IntGauge};

pub static TOTAL_BYTES_READ: Lazy<IntCounter> = Lazy::new(|| {
    tari_metrics::register_int_counter("comms::substream::total_bytes_read", "The total inbound bytes").unwrap()
});

pub static TOTAL_BYTES_WRITTEN: Lazy<IntCounter> = Lazy::new(|| {
    tari_metrics::register_int_counter("comms::substream::total_bytes_written", "The total outbound bytes").unwrap()
});

pub static INBOUND_SUBSTREAMS_DROPPED: Lazy<IntCounter> = Lazy::new(|| {
    tari_metrics::register_int_counter(
        "comms::substream::inbound_dropped",
        "The number of inbound substreams reset because too many were waiting to be accepted",
    )
    .unwrap()
});

pub static QUEUED_INBOUND_SUBSTREAMS: Lazy<IntGauge> = Lazy::new(|| {
    tari_metrics::register_int_gauge(
        "comms::substream::queued_inbound",
        "The number of inbound substreams waiting to be accepted, across all connections",
    )
    .unwrap()
});

/// The number of connections whose socket has not accepted a write for at least `WRITE_STALL_REPORT_AFTER`
pub fn write_stalled_connections() -> IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "comms::multiplexing::write_stalled_connections",
            "The number of connections whose socket has not accepted a write for at least 30 seconds",
        )
        .unwrap()
    });

    METER.clone()
}

/// The duration, in seconds, of write stalls that lasted at least `WRITE_STALL_REPORT_AFTER`
pub fn write_stall_duration() -> Histogram {
    static METER: Lazy<Histogram> = Lazy::new(|| {
        let opts = HistogramOpts::new(
            "comms::multiplexing::write_stall_duration",
            "The duration in seconds of write stalls that lasted at least 30 seconds",
        )
        .buckets(vec![30.0, 45.0, 60.0, 90.0, 120.0, 180.0]);
        let histogram = Histogram::with_opts(opts).unwrap();
        tari_metrics::register(histogram.clone()).unwrap();
        histogram
    });

    METER.clone()
}

/// The number of inbound substreams reset per connection because too many were waiting to be accepted, observed when
/// the connection closes
pub fn inbound_dropped_per_connection() -> Histogram {
    static METER: Lazy<Histogram> = Lazy::new(|| {
        let opts = HistogramOpts::new(
            "comms::multiplexing::inbound_dropped_per_connection",
            "The number of inbound substreams reset per connection because too many were waiting to be accepted",
        )
        .buckets(vec![0.0, 1.0, 10.0, 100.0, 1000.0, 10000.0]);
        let histogram = Histogram::with_opts(opts).unwrap();
        tari_metrics::register(histogram.clone()).unwrap();
        histogram
    });

    METER.clone()
}
