// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::atomic::{AtomicUsize, Ordering};

/// State for the LivenessService.
#[derive(Default)]
pub struct LivenessState {
    pings_received: AtomicUsize,
    pongs_received: AtomicUsize,

    pings_sent: AtomicUsize,
    pongs_sent: AtomicUsize,
}

impl LivenessState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn inc_pings_sent(&self) -> usize {
        self.pings_sent.fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_pongs_sent(&self) -> usize {
        self.pongs_sent.fetch_add(1, Ordering::Relaxed)
    }

    pub fn pings_sent(&self) -> usize {
        self.pings_sent.load(Ordering::Relaxed)
    }

    pub fn pongs_sent(&self) -> usize {
        self.pongs_sent.load(Ordering::Relaxed)
    }

    pub fn inc_pings_received(&self) -> usize {
        self.pings_received.fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_pongs_received(&self) -> usize {
        self.pongs_received.fetch_add(1, Ordering::Relaxed)
    }

    pub fn pings_received(&self) -> usize {
        self.pings_received.load(Ordering::Relaxed)
    }

    pub fn pongs_received(&self) -> usize {
        self.pongs_received.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new() {
        let state = LivenessState::new();
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn getters() {
        let state = LivenessState::new();
        state.pings_received.store(5, Ordering::SeqCst);
        assert_eq!(state.pings_received(), 5);
        assert_eq!(state.pongs_received(), 0);
        assert_eq!(state.pings_sent(), 0);
        assert_eq!(state.pongs_sent(), 0);
    }

    #[test]
    fn inc_pings_sent() {
        let state = LivenessState::new();
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pings_sent(), 0);
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pongs_sent() {
        let state = LivenessState::new();
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pongs_sent(), 0);
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pings_received() {
        let state = LivenessState::new();
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pings_received(), 0);
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pongs_received() {
        let state = LivenessState::new();
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pongs_received(), 0);
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 1);
    }
}
