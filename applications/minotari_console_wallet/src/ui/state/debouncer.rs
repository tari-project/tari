// Copyright 2020. The Tari Project
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

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
use minotari_wallet::output_manager_service::handle::OutputManagerHandle;
use tokio::{
    sync::{broadcast, RwLock},
    time,
    time::MissedTickBehavior,
};

use crate::ui::state::AppStateInner;

const LOG_TARGET: &str = "wallet::console_wallet::debouncer";

#[derive(Clone)]
pub(crate) struct BalanceEnquiryDebouncer {
    app_state_inner: Arc<RwLock<AppStateInner>>,
    output_manager_service: OutputManagerHandle,
    balance_enquiry_cooldown_period: Duration,
    tx: broadcast::Sender<()>,
}

impl BalanceEnquiryDebouncer {
    pub fn new(
        app_state_inner: Arc<RwLock<AppStateInner>>,
        balance_enquiry_cooldown_period: Duration,
        output_manager_service: OutputManagerHandle,
    ) -> Self {
        // This channel must only be size 1; the debouncer will ensure that the balance is updated timeously
        let (tx, _) = broadcast::channel(1);
        Self {
            app_state_inner,
            output_manager_service,
            balance_enquiry_cooldown_period,
            tx,
        }
    }

    pub async fn run(mut self) {
        let balance_enquiry_events = &mut self.tx.subscribe();
        let mut shutdown_signal = self.app_state_inner.read().await.get_shutdown_signal();
        let mut interval = time::interval(self.balance_enquiry_cooldown_period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tokio::pin!(interval);

        debug!(target: LOG_TARGET, "Balance enquiry debouncer starting");
        if let Ok(balance) = self.output_manager_service.get_balance().await {
            trace!(
                target: LOG_TARGET,
                "Initial balance: available {}, time-locked {}, incoming {}, outgoing {}",
                balance.available_balance,
                balance.time_locked_balance.unwrap_or(0.into()),
                balance.pending_incoming_balance,
                balance.pending_outgoing_balance
            );
            let mut inner = self.app_state_inner.write().await;
            if let Err(e) = inner.refresh_balance(balance).await {
                warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
            }
        }
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(result) = time::timeout(
                        self.balance_enquiry_cooldown_period,
                        balance_enquiry_events.recv()
                    ).await {
                        if let Err(broadcast::error::RecvError::Lagged(n)) = result {
                            trace!(target: LOG_TARGET, "Balance enquiry debouncer lagged {} update requests", n);
                        }
                        match result {
                            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {
                                let start_time = Instant::now();
                                match self.output_manager_service.get_balance().await {
                                    Ok(balance) => {
                                        trace!(
                                            target: LOG_TARGET,
                                            "Updating balance ({} ms): available {}, incoming {}, outgoing {}",
                                            start_time.elapsed().as_millis(),
                                            balance.available_balance,
                                            balance.pending_incoming_balance,
                                            balance.pending_outgoing_balance
                                        );
                                        let mut inner = self.app_state_inner.write().await;
                                        if let Err(e) = inner.refresh_balance(balance).await {
                                            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        warn!(target: LOG_TARGET, "Could not obtain balance ({})", e);
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!(
                                    target: LOG_TARGET,
                                    "Balance enquiry debouncer shutting down because the channel was closed"
                                );
                                break;
                            }
                        }
                    }
                },
                _ = shutdown_signal.wait() => {
                    info!(
                        target: LOG_TARGET,
                        "Balance enquiry debouncer shutting down because the shutdown signal was received"
                    );
                    break;
                },
            }
        }
    }

    pub fn get_sender(self) -> broadcast::Sender<()> {
        self.tx
    }
}
