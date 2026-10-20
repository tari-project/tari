// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::{Mutex, Once};

static LOGS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Ok(mut list) = LOGS.lock() {
            list.push((record.level(), record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = log::set_boxed_logger(Box::new(CaptureLogger));
        log::set_max_level(log::LevelFilter::Trace);
    });
}

pub fn get_logs() -> Vec<(log::Level, String)> {
    LOGS.lock().unwrap().clone()
}
