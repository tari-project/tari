//  Copyright 2019 The Tari Project
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

//! # Inbound Message Service
//!
//! The inbound message service is responsible for receiving messages from the active [PeerConnection]s
//! and fair-dealing them to one of the worker threads for processing.
//!
//! Worker thread will perform the following tasks:
//!
//! 1. Validate the message signature against the sender's public key.
//! 2. Attempt to decrypt the message with a ECDH shared secret if the MessageFlags::ENCRYPTED flag is set.
//! 3. Check the destination [NodeId] or [CommsPublicKey]
//! 3. Should steps 1-3 fail, forward or discard the message as necessary. See [comms_msg_handlers].
//! 4. Otherwise, dispatch the message to one of the configured message broker routes. See [InboundMessageBroker]
//!
//! [PeerConnection]: ../connection/peer_connection/index.html
//! [comms_msg_handlers]: ./comms_msg_handlers/struct.InboundMessageServiceResolver.html
//! [InboundMessageBroker]: ./inbound_message_broker/struct.InboundMessageBroker.html
pub mod error;
pub mod inbound_message_service;
pub mod message_cache;

pub use self::message_cache::{MessageCache, MessageCacheConfig};
