// Copyright 2023, The Tari Project
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

use std::ops::Deref;

use log::{debug, info, trace};
use tari_contacts::contacts_service::handle::{ContactsLivenessData, ContactsLivenessEvent, ContactsServiceHandle};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "chat_ffi::callback_handler";

pub(crate) type CallbackContactStatusChange = unsafe extern "C" fn(*mut ContactsLivenessData);

#[derive(Clone)]
pub struct CallbackHandler {
    contacts_service_handle: ContactsServiceHandle,
    callback_contact_status_change: CallbackContactStatusChange,
    shutdown: ShutdownSignal,
}

impl CallbackHandler {
    pub fn new(
        contacts_service_handle: ContactsServiceHandle,
        shutdown: ShutdownSignal,
        callback_contact_status_change: CallbackContactStatusChange,
    ) -> Self {
        Self {
            contacts_service_handle,
            shutdown,
            callback_contact_status_change,
        }
    }

    pub(crate) async fn start(&mut self) {
        let mut liveness_events = self.contacts_service_handle.get_contacts_liveness_event_stream();

        loop {
            tokio::select! {
                event = liveness_events.recv() => {
                    match event {
                        Ok(liveness_event) => {
                            match liveness_event.deref() {
                                ContactsLivenessEvent::StatusUpdated(data) => {
                                    trace!(target: LOG_TARGET,
                                        "FFI Callback monitor received Contact Status Updated event"
                                    );
                                    self.trigger_contact_status_change(data.deref().clone());
                                }
                                ContactsLivenessEvent::NetworkSilence => {},
                            }
                        },
                        Err(_) => { debug!(target: LOG_TARGET, "FFI Callback monitor had an error with contacts liveness")}
                    }
                },
                _ = self.shutdown.wait() => {
                    info!(target: LOG_TARGET, "ChatFFI Callback Handler shutting down because the shutdown signal was received");
                    break;
                },
            }
        }
    }

    fn trigger_contact_status_change(&mut self, data: ContactsLivenessData) {
        debug!(
            target: LOG_TARGET,
            "Calling Contacts Liveness Data Updated callback function for contact {}",
            data.address(),
        );
        unsafe {
            (self.callback_contact_status_change)(Box::into_raw(Box::new(data)));
        }
    }
}
