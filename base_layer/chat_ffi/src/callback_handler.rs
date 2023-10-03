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

use std::{convert::TryFrom, ops::Deref};

use log::{debug, error, info, trace};
use tari_contacts::contacts_service::{
    handle::{ContactsLivenessData, ContactsLivenessEvent, ContactsServiceHandle},
    types::{Confirmation, Message, MessageDispatch},
};
use tari_shutdown::ShutdownSignal;

use crate::types::ChatFFIContactsLivenessData;

const LOG_TARGET: &str = "chat_ffi::callback_handler";

pub(crate) type CallbackContactStatusChange = unsafe extern "C" fn(*mut ChatFFIContactsLivenessData);
pub(crate) type CallbackMessageReceived = unsafe extern "C" fn(*mut Message);
pub(crate) type CallbackDeliveryConfirmationReceived = unsafe extern "C" fn(*mut Confirmation);
pub(crate) type CallbackReadConfirmationReceived = unsafe extern "C" fn(*mut Confirmation);

#[derive(Clone)]
pub struct CallbackHandler {
    contacts_service_handle: ContactsServiceHandle,
    callback_contact_status_change: CallbackContactStatusChange,
    callback_message_received: CallbackMessageReceived,
    callback_delivery_confirmation_received: CallbackDeliveryConfirmationReceived,
    callback_read_confirmation_received: CallbackReadConfirmationReceived,
    shutdown: ShutdownSignal,
}

impl CallbackHandler {
    pub fn new(
        contacts_service_handle: ContactsServiceHandle,
        shutdown: ShutdownSignal,
        callback_contact_status_change: CallbackContactStatusChange,
        callback_message_received: CallbackMessageReceived,
        callback_delivery_confirmation_received: CallbackDeliveryConfirmationReceived,
        callback_read_confirmation_received: CallbackReadConfirmationReceived,
    ) -> Self {
        Self {
            contacts_service_handle,
            shutdown,
            callback_contact_status_change,
            callback_message_received,
            callback_delivery_confirmation_received,
            callback_read_confirmation_received,
        }
    }

    pub(crate) async fn start(&mut self) {
        let mut liveness_events = self.contacts_service_handle.get_contacts_liveness_event_stream();
        let mut chat_messages = self.contacts_service_handle.get_messages_event_stream();

        loop {
            tokio::select! {
                rec_message = chat_messages.recv() => {
                    match rec_message {
                        Ok(message_dispatch) => {
                            trace!(target: LOG_TARGET, "FFI Callback monitor received a new MessageDispatch");
                            match message_dispatch.deref() {
                                MessageDispatch::Message(m) => {
                                    trace!(target: LOG_TARGET, "FFI Callback monitor received a new Message");
                                    self.trigger_message_received(m.clone());
                                }
                                MessageDispatch::DeliveryConfirmation(c) => {
                                    trace!(target: LOG_TARGET, "FFI Callback monitor received a new Delivery Confirmation");
                                    self.trigger_delivery_confirmation_received(c.clone());
                                },
                                MessageDispatch::ReadConfirmation(c) => {
                                    trace!(target: LOG_TARGET, "FFI Callback monitor received a new Read Confirmation");
                                    self.trigger_read_confirmation_received(c.clone());
                                }
                            };
                        },
                        Err(_) => { debug!(target: LOG_TARGET, "FFI Callback monitor had an error receiving new messages")}
                    }
                },

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
            "Calling ContactStatusChanged callback function for contact {}",
            data.address(),
        );

        match ChatFFIContactsLivenessData::try_from(data) {
            Ok(data) => unsafe {
                (self.callback_contact_status_change)(Box::into_raw(Box::new(data)));
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Error processing contacts liveness data received callback: {}", e)
            },
        }
    }

    fn trigger_message_received(&mut self, message: Message) {
        debug!(
            target: LOG_TARGET,
            "Calling MessageReceived callback function for sender {}",
            message.address,
        );

        unsafe {
            (self.callback_message_received)(Box::into_raw(Box::new(message)));
        }
    }

    fn trigger_delivery_confirmation_received(&mut self, confirmation: Confirmation) {
        debug!(
            target: LOG_TARGET,
            "Calling DeliveryConfirmationReceived callback function for message {:?}",
            confirmation.message_id,
        );

        unsafe {
            (self.callback_delivery_confirmation_received)(Box::into_raw(Box::new(confirmation)));
        }
    }

    fn trigger_read_confirmation_received(&mut self, confirmation: Confirmation) {
        debug!(
            target: LOG_TARGET,
            "Calling ReadConfirmationReceived callback function for message {:?}",
            confirmation.message_id,
        );

        unsafe {
            (self.callback_read_confirmation_received)(Box::into_raw(Box::new(confirmation)));
        }
    }
}
