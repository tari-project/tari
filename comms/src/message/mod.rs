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

use crate::peer_manager::node_id::NodeId;
use bitflags::*;
use serde::{Deserialize, Serialize};

mod domain_message_context;
mod envelope;
mod error;
mod message;
mod message_context;
mod message_data;
pub mod p2p;

pub use self::{
    domain_message_context::*,
    envelope::*,
    error::MessageError,
    message::{Message, MessageHeader},
    message_context::MessageContext,
    message_data::*,
};

/// Represents a single message frame.
pub type Frame = Vec<u8>;
/// Represents a collection of frames which make up a multipart message.
pub type FrameSet = Vec<Frame>;

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct MessageFlags: u8 {
        const NONE = 0b00000000;
        const ENCRYPTED = 0b00000001;
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum NodeDestination<P> {
    Unknown,
    PublicKey(P),
    NodeId(NodeId),
}
