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
use tari_metrics::{IntCounter, IntCounterVec, IntGauge};

pub fn num_sessions() -> IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "comms::messaging::num_sessions",
            "The number of active messaging sessions",
        )
        .unwrap()
    });

    METER.clone()
}

pub fn outbound_message_count() -> IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms::messaging::outbound_message_count",
            "The number of handshakes per peer",
        )
        .unwrap()
    });

    METER.clone()
}

pub fn inbound_message_count() -> IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms::messaging::inbound_message_count",
            "The number of handshakes per peer",
        )
        .unwrap()
    });

    METER.clone()
}

pub fn error_count() -> IntCounter {
    static METER: Lazy<IntCounter> =
        Lazy::new(|| tari_metrics::register_int_counter("comms::messaging::errors", "The number of errors").unwrap());

    METER.clone()
}

/// Outbound messages that failed without being sent, by reason:
/// - `queue_full`: the peer's queue held the most messages it may
/// - `queue_bytes_full`: the message would take the peer's queue over its byte budget
/// - `total_bytes_full`: the message would take all outbound queues over the node-wide byte budget
/// - `dialing_bytes_full`: the message would take the queues still dialing over their byte budget
/// - `dialing_queue_bytes_full`: the message would take a queue that is still dialing over its own byte budget
/// - `too_large`: the message is larger than a frame
/// - `retry_queue_full`: the message could not be put on the retry queue
/// - `max_retries`: the message was already retried `MAX_SEND_RETRIES` times, each time because the peer closed or
///   reset the substream
/// - `max_retries_connection_lost`: the message was already retried `MAX_SEND_RETRIES` times, at least once because the
///   connection to the peer went away
/// - `expired`: the message was past its deadline
/// - `send_timeout`: a write to the peer timed out (the peer stopped reading)
/// - `queued_behind_timeout`: the message was queued behind one whose write timed out, and was never attempted
///
/// There is deliberately no peer label (cardinality); the `warn!` logged for a full node-wide or dialing budget names
/// the peer holding the most bytes.
pub fn outbound_dropped(reason: &str) -> IntCounter {
    static METER: Lazy<IntCounterVec> = Lazy::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::messaging::outbound_dropped_total",
            "The number of outbound messages that failed without being sent, by reason",
            &["reason"],
        )
        .unwrap()
    });

    METER.with_label_values(&[reason])
}
