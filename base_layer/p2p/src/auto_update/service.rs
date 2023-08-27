//  Copyright 2021, The Taiji Project
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

use std::env::consts;

use futures::{future::Either, stream, StreamExt};
use log::*;
use taiji_common::configuration::bootstrap::ApplicationType;
use taiji_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time,
    time::MissedTickBehavior,
};
use tokio_stream::wrappers;

use crate::{
    auto_update,
    auto_update::{AutoUpdateConfig, SoftwareUpdate, Version},
};

const LOG_TARGET: &str = "p2p::auto_update";

/// A watch notifier that contains the latest software update, if any
pub type SoftwareUpdateNotifier = watch::Receiver<Option<SoftwareUpdate>>;

#[derive(Clone)]
pub struct SoftwareUpdaterHandle {
    update_notifier: SoftwareUpdateNotifier,
    request_tx: mpsc::Sender<oneshot::Sender<Option<SoftwareUpdate>>>,
}

impl SoftwareUpdaterHandle {
    /// Returns watch notifier that emits a value whenever a new software update is detected.
    /// First the current SoftwareUpdate (if any) is emitted. Thereafter, only software updates with a greater version
    /// number are emitted.
    pub fn update_notifier(&self) -> &SoftwareUpdateNotifier {
        &self.update_notifier
    }

    /// Returns the latest update or None if the updater has not retrieved the latest update yet.
    pub fn latest_update(&self) -> watch::Ref<'_, Option<SoftwareUpdate>> {
        self.update_notifier.borrow()
    }

    /// Returns watch notifier that triggers after a check for software updates
    pub async fn check_for_updates(&mut self) -> Option<SoftwareUpdate> {
        let (tx, rx) = oneshot::channel();
        // If this is cancelled (e.g due to shutdown being triggered), return None (no update)
        self.request_tx.send(tx).await.ok()?;
        rx.await.ok().flatten()
    }
}

#[derive(Debug, Clone)]
pub struct SoftwareUpdaterService {
    application: ApplicationType,
    current_version: Version,
    config: AutoUpdateConfig,
}

impl SoftwareUpdaterService {
    pub fn new(application: ApplicationType, current_version: Version, config: AutoUpdateConfig) -> Self {
        Self {
            application,
            current_version,
            config,
        }
    }

    async fn run(
        self,
        mut request_rx: mpsc::Receiver<oneshot::Sender<Option<SoftwareUpdate>>>,
        notifier: watch::Sender<Option<SoftwareUpdate>>,
        new_update_notification: watch::Receiver<Option<SoftwareUpdate>>,
    ) {
        let mut interval_or_never = match self.config.check_interval {
            Some(interval) => {
                if interval.is_zero() {
                    Either::Right(stream::empty())
                } else {
                    let mut interval = time::interval(interval);
                    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                    Either::Left(wrappers::IntervalStream::new(interval))
                }
            },
            None => Either::Right(stream::empty()),
        };

        loop {
            let last_version = new_update_notification.borrow().clone();

            let maybe_update = tokio::select! {
                Some(reply) = request_rx.recv() => {
                    let maybe_update = self.check_for_updates().await;
                    let _result = reply.send(maybe_update.clone());
                    maybe_update
               },

               Some(_) = interval_or_never.next() => {
                    // Periodically, check for updates if configured to do so.
                    // If an update is found the new update notifier will be triggered and any listeners notified
                    self.check_for_updates().await
                }
            };

            // Only notify of new or newer updates
            if let Some(update) = maybe_update {
                if last_version
                    .as_ref()
                    .map(|up| up.version() < update.version())
                    .unwrap_or(true)
                {
                    let _result = notifier.send(Some(update.clone()));
                }
            }
        }
    }

    async fn check_for_updates(&self) -> Option<SoftwareUpdate> {
        log::info!(
            target: LOG_TARGET,
            "Checking for updates ({})...",
            self.config.update_uris.join(", ")
        );
        if !self.config.is_update_enabled() {
            warn!(
                target: LOG_TARGET,
                "Check for updates has been called but auto update has been disabled in the config"
            );
            return None;
        }

        let arch = format!("{}-{}", consts::OS, consts::ARCH);

        match auto_update::check_for_updates(self.application, &arch, &self.current_version, self.config.clone()).await
        {
            Ok(Some(update)) => {
                log::info!(target: LOG_TARGET, "Update found {}", update);
                Some(update)
            },
            Ok(None) => {
                log::info!(
                    target: LOG_TARGET,
                    "No new update found. Current: {} {} {}",
                    self.application,
                    self.current_version,
                    arch
                );
                None
            },
            Err(err) => {
                log::warn!(target: LOG_TARGET, "Unable to check for software updates: {}", err);
                None
            },
        }
    }
}

#[async_trait]
impl ServiceInitializer for SoftwareUpdaterService {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Software Update Service");
        let service = self.clone();

        let (notifier, new_update_notif) = watch::channel(None);
        let (request_tx, request_rx) = mpsc::channel(1);

        context.register_handle(SoftwareUpdaterHandle {
            update_notifier: new_update_notif.clone(),
            request_tx,
        });
        context.spawn_until_shutdown(move |_| service.run(request_rx, notifier, new_update_notif));
        debug!(target: LOG_TARGET, "Software Update Service Initialized");
        Ok(())
    }
}
