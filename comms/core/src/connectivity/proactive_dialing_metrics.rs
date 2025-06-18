//  Copyright 2020, The Tari Project
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

#[cfg(feature = "metrics")]
use once_cell::sync::Lazy;
#[cfg(feature = "metrics")]
use tari_metrics::{Gauge, Histogram, IntCounter, IntGauge};

#[cfg(feature = "metrics")]
pub fn proactive_dials_attempted() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_proactive_dials_attempted_total",
            "Total number of proactive dial attempts initiated",
        )
        .unwrap()
    });
    &METER
}

pub fn proactive_dials_successful() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_proactive_dials_successful_total",
            "Total number of successful proactive dial attempts",
        )
        .unwrap()
    });
    &METER
}

pub fn proactive_dials_failed() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_proactive_dials_failed_total",
            "Total number of failed proactive dial attempts",
        )
        .unwrap()
    });
    &METER
}

pub fn circuit_breaker_state_changes() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_circuit_breaker_state_changes_total",
            "Total number of circuit breaker state changes",
        )
        .unwrap()
    });
    &METER
}

pub fn circuit_breaker_open_peers() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "comms_connectivity_circuit_breaker_open_peers",
            "Number of peers with open circuit breakers",
        )
        .unwrap()
    });
    &METER
}

pub fn target_connections_achieved() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "comms_connectivity_target_connections_achieved",
            "Whether target connection count has been achieved (1 = yes, 0 = no)",
        )
        .unwrap()
    });
    &METER
}

pub fn available_peer_candidates() -> &'static IntGauge {
    static METER: Lazy<IntGauge> = Lazy::new(|| {
        tari_metrics::register_int_gauge(
            "comms_connectivity_available_peer_candidates",
            "Number of available peer candidates for dialing",
        )
        .unwrap()
    });
    &METER
}

pub fn average_peer_health_score() -> &'static Gauge {
    static METER: Lazy<Gauge> = Lazy::new(|| {
        tari_metrics::register_gauge(
            "comms_connectivity_average_peer_health_score",
            "Average health score of known peers (0.0 to 1.0)",
        )
        .unwrap()
    });
    &METER
}

pub fn peer_discovery_attempts() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_peer_discovery_attempts_total",
            "Total number of peer discovery attempts",
        )
        .unwrap()
    });
    &METER
}

pub fn peer_discovery_peers_found() -> &'static IntCounter {
    static METER: Lazy<IntCounter> = Lazy::new(|| {
        tari_metrics::register_int_counter(
            "comms_connectivity_peer_discovery_peers_found_total",
            "Total number of peers found through discovery",
        )
        .unwrap()
    });
    &METER
}

pub fn proactive_dialing_execution_time() -> &'static Histogram {
    static METER: Lazy<Histogram> = Lazy::new(|| {
        tari_metrics::register_histogram(
            "comms_connectivity_proactive_dialing_execution_time_seconds",
            "Time taken to execute proactive dialing logic",
        )
        .unwrap()
    });
    &METER
}

pub fn connection_success_rate() -> &'static Gauge {
    static METER: Lazy<Gauge> = Lazy::new(|| {
        tari_metrics::register_gauge(
            "comms_connectivity_connection_success_rate",
            "Recent connection success rate (0.0 to 1.0)",
        )
        .unwrap()
    });
    &METER
}

pub fn dialing_multiplier_applied() -> &'static Gauge {
    static METER: Lazy<Gauge> = Lazy::new(|| {
        tari_metrics::register_gauge(
            "comms_connectivity_dialing_multiplier_applied",
            "Actual dialing multiplier applied based on success rate",
        )
        .unwrap()
    });
    &METER
}



// Helper functions for easy metric updates
#[cfg(feature = "metrics")]
pub fn increment_proactive_dials_attempted() {
    proactive_dials_attempted().inc();
}

#[cfg(feature = "metrics")]
pub fn increment_proactive_dials_successful() {
    proactive_dials_successful().inc();
}

#[cfg(feature = "metrics")]
pub fn increment_proactive_dials_failed() {
    proactive_dials_failed().inc();
}

#[cfg(feature = "metrics")]
pub fn increment_circuit_breaker_state_changes() {
    circuit_breaker_state_changes().inc();
}

#[cfg(feature = "metrics")]
pub fn set_circuit_breaker_open_peers(count: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let value = count as i64;
    circuit_breaker_open_peers().set(value);
}

#[cfg(feature = "metrics")]
pub fn set_target_connections_achieved(achieved: bool) {
    target_connections_achieved().set(if achieved { 1 } else { 0 });
}

#[cfg(feature = "metrics")]
pub fn set_available_peer_candidates(count: usize) {
    #[allow(clippy::cast_possible_wrap)]
    let value = count as i64;
    available_peer_candidates().set(value);
}

#[cfg(feature = "metrics")]
pub fn set_average_peer_health_score(score: f32) {
    average_peer_health_score().set(f64::from(score));
}

#[cfg(feature = "metrics")]
pub fn increment_peer_discovery_attempts() {
    peer_discovery_attempts().inc();
}

#[cfg(feature = "metrics")]
pub fn increment_peer_discovery_peers_found(count: usize) {
    peer_discovery_peers_found().inc_by(count as u64);
}

#[cfg(feature = "metrics")]
pub fn observe_proactive_dialing_execution_time(duration: std::time::Duration) {
    proactive_dialing_execution_time().observe(duration.as_secs_f64());
}

#[cfg(feature = "metrics")]
pub fn set_connection_success_rate(rate: f32) {
    connection_success_rate().set(f64::from(rate));
}

#[cfg(feature = "metrics")]
pub fn set_dialing_multiplier_applied(multiplier: f32) {
    dialing_multiplier_applied().set(f64::from(multiplier));
}

// Stub functions when metrics feature is disabled
#[cfg(not(feature = "metrics"))]
pub fn increment_proactive_dials_attempted() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_proactive_dials_successful() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_proactive_dials_failed() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_circuit_breaker_state_changes() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_peer_discovery_attempts() {}

#[cfg(not(feature = "metrics"))]
pub fn increment_peer_discovery_peers_found(_count: usize) {}

#[cfg(not(feature = "metrics"))]
pub fn set_circuit_breaker_open_peers(_count: usize) {}

#[cfg(not(feature = "metrics"))]
pub fn set_target_connections_achieved(_achieved: bool) {}

#[cfg(not(feature = "metrics"))]
pub fn set_available_peer_candidates(_count: usize) {}

#[cfg(not(feature = "metrics"))]
pub fn set_average_peer_health_score(_score: f32) {}

#[cfg(not(feature = "metrics"))]
pub fn set_connection_success_rate(_rate: f32) {}

#[cfg(not(feature = "metrics"))]
pub fn set_dialing_multiplier_applied(_multiplier: f32) {}

#[cfg(not(feature = "metrics"))]
pub fn observe_proactive_dialing_execution_time(_duration: std::time::Duration) {}


