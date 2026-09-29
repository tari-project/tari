//  Copyright 2026, The Tari Project
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

//! A fixed bound on the number of inbound gossip messages (transactions or blocks) that a service has accepted but not
//! yet finished handling. Each accepted message holds a permit for as long as its handling task runs; once the bound is
//! reached, further messages are dropped (and logged at a limited rate) instead of queueing without limit.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// How often, at most, a summary of dropped messages is logged
const DROP_LOG_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) struct InboundBackpressure {
    pending: Arc<Semaphore>,
    capacity: usize,
    kind: &'static str,
    log_target: &'static str,
    dropped_since_log: u64,
    last_logged: Option<Instant>,
}

impl InboundBackpressure {
    pub fn new(capacity: usize, kind: &'static str, log_target: &'static str) -> Self {
        Self {
            pending: Arc::new(Semaphore::new(capacity)),
            capacity,
            kind,
            log_target,
            dropped_since_log: 0,
            last_logged: None,
        }
    }

    /// Try to accept an inbound message. Returns a permit to be held by the task handling the message, or `None` if
    /// too many messages are already pending, in which case the message must be dropped (no ban: an honest but busy
    /// peer can cause this too).
    pub fn try_accept(&mut self) -> Option<OwnedSemaphorePermit> {
        match self.pending.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => {
                self.dropped_since_log = self.dropped_since_log.saturating_add(1);
                if self.last_logged.is_none_or(|t| t.elapsed() >= DROP_LOG_INTERVAL) {
                    warn!(
                        target: self.log_target,
                        "{} pending inbound {} message(s) already being handled; dropped {} message(s) since the last \
                         report",
                        self.capacity,
                        self.kind,
                        self.dropped_since_log
                    );
                    self.dropped_since_log = 0;
                    self.last_logged = Some(Instant::now());
                }
                None
            },
        }
    }

    /// The number of messages that can still be accepted
    #[cfg(test)]
    pub fn available(&self) -> usize {
        self.pending.available_permits()
    }

    /// The underlying semaphore, for tests
    #[cfg(test)]
    pub fn semaphore(&self) -> Arc<Semaphore> {
        self.pending.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn accepts_up_to_capacity_then_drops() {
        let mut backpressure = InboundBackpressure::new(2, "test", "test");
        let a = backpressure.try_accept().unwrap();
        let b = backpressure.try_accept().unwrap();
        assert!(backpressure.try_accept().is_none());
        assert!(backpressure.try_accept().is_none());
        assert_eq!(backpressure.available(), 0);
        drop(a);
        assert_eq!(backpressure.available(), 1);
        let _c = backpressure.try_accept().unwrap();
        drop(b);
        assert_eq!(backpressure.available(), 1);
    }
}
