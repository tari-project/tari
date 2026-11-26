// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(feature = "server")]
pub mod server;

use std::sync::{Arc, PoisonError, RwLock};

use once_cell::sync::Lazy;
use prometheus::opts;
pub use prometheus::{
    Counter,
    CounterVec,
    Error,
    Gauge,
    GaugeVec,
    Histogram,
    HistogramOpts,
    HistogramTimer,
    HistogramVec,
    IntCounter,
    IntCounterVec,
    IntGauge,
    IntGaugeVec,
    Registry,
    core::Collector,
    proto,
};

static DEFAULT_REGISTRY: Lazy<Arc<RwLock<Registry>>> = Lazy::new(|| Arc::new(RwLock::new(Registry::default())));

/// Sets the global default registry.
///
/// This should be set once by the application. Libraries should never set this.
pub fn set_default_registry(registry: Registry) {
    *DEFAULT_REGISTRY.write().unwrap_or_else(PoisonError::into_inner) = registry;
}

pub fn get_default_registry() -> Registry {
    DEFAULT_REGISTRY.read().unwrap_or_else(PoisonError::into_inner).clone()
}

pub fn register<C: Collector + 'static>(c: C) -> prometheus::Result<()> {
    get_default_registry().register(Box::new(c))
}

pub fn register_counter(name: &str, help: &str) -> prometheus::Result<Counter> {
    let counter = prometheus::Counter::new(name, help)?;
    register(counter.clone())?;
    Ok(counter)
}

pub fn register_counter_vec(name: &str, help: &str, label_names: &[&str]) -> prometheus::Result<CounterVec> {
    let counter = prometheus::CounterVec::new(opts!(name, help), label_names)?;
    register(counter.clone())?;
    Ok(counter)
}

pub fn register_gauge(name: &str, help: &str) -> prometheus::Result<Gauge> {
    let gauge = prometheus::Gauge::new(name, help)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_gauge_vec(name: &str, help: &str, label_names: &[&str]) -> prometheus::Result<GaugeVec> {
    let gauge = prometheus::GaugeVec::new(opts!(name, help), label_names)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_int_gauge_vec(name: &str, help: &str, label_names: &[&str]) -> prometheus::Result<IntGaugeVec> {
    let gauge = prometheus::IntGaugeVec::new(opts!(name, help), label_names)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_int_counter(name: &str, help: &str) -> prometheus::Result<IntCounter> {
    let gauge = prometheus::IntCounter::new(name, help)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_int_counter_vec(name: &str, help: &str, label_names: &[&str]) -> prometheus::Result<IntCounterVec> {
    let gauge = prometheus::IntCounterVec::new(opts!(name, help), label_names)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_int_gauge(name: &str, help: &str) -> prometheus::Result<IntGauge> {
    let gauge = prometheus::IntGauge::new(name, help)?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_histogram(name: &str, help: &str) -> prometheus::Result<Histogram> {
    let gauge = prometheus::Histogram::with_opts(HistogramOpts::new(name, help))?;
    register(gauge.clone())?;
    Ok(gauge)
}

pub fn register_histogram_vec(name: &str, help: &str, label_names: &[&str]) -> prometheus::Result<HistogramVec> {
    let gauge = prometheus::HistogramVec::new(HistogramOpts::new(name, help), label_names)?;
    register(gauge.clone())?;
    Ok(gauge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_counter_and_counter_vec_in_default_registry() {
        set_default_registry(Registry::new());
        let counter = register_counter("test_counter_total", "test counter").unwrap();
        counter.inc_by(2.5);
        assert!((counter.get() - 2.5).abs() < f64::EPSILON);

        let vec = register_counter_vec("test_counter_vec_total", "test counter vec", &["kind"]).unwrap();
        vec.with_label_values(&["alpha"]).inc();
        assert!((vec.with_label_values(&["alpha"]).get() - 1.0).abs() < f64::EPSILON);
    }
}
