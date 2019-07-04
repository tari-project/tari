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

//! # Outbound Message Service (OMS)
//!
//! Responsible for sending messages on the peer-to-peer network.
//!
//! In order to send a message the OMS:
//!
//! - evaluates and selects [Peer]'s according to the given [BroadcastStrategy],
//! - constructs, signs and optionally encrypts a [MessageEnvelope] for each selected [Peer], and
//! - forwards each constructed message frame to the [OutboundMessagePool] (OMP).
//!
//! # Broadcast Strategy
//!
//! Represents a strategy for selecting known [Peer]s from the [PeerManager].
//! See [BroadcastStrategy] for more details.
//!
//! # Outbound Message Pool (OMP)
//!
//! Responsible for reliably sending messages to [Peer]s.
//!
//! The OMP reads from an [0MQ inproc] message queue. Each message received on this queue represents
//! a message which should be delivered to a single peer. A message is fair-dealt to a worker for
//! processing. The worker thread attempts to establish a [PeerConnection] to the given [Peer]
//! using the [ConnectionManager]. Once established, it uses the connection to send the
//! message. Once sent, it discards the message. If, for whatever reason, the message fails
//! to send, the message will be requeued and will try again later. If the message fails after
//! a configured number of attempts, the message is discarded.
//!
//! [BroadcastStrategy]: ./broadcast_strategy/enum.BroadcastStrategy.html
//! [MessageEnvelope]: ../message/struct.MessageEnvelope.html
//! [Peer]: ../peer_manager/peer/struct.Peer.html
//! [OutboundMessagePool]: ./outbound_message_pool/struct.OutboundMessagePool.html
//! [PeerConnection]: ../connection/peer_connection/index.html
//! [ConnectionManager]: ../connection_manager/index.html
//! [PeerManager]: ../peer_manager/index.html
//! [0MQ inproc]: http://api.zeromq.org/2-1:zmq-inproc

pub mod broadcast_strategy;
pub mod error;
pub mod outbound_message_pool;
pub mod outbound_message_service;

pub use self::{
    broadcast_strategy::BroadcastStrategy,
    error::OutboundError,
    outbound_message_pool::{OutboundMessage, OutboundMessagePool},
};
