// Copyright 2019, The Tari Project
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

use crate::proto::envelope::Network;
use std::fmt;
use tari_utilities::hex::Hex;

#[path = "tari.dht.envelope.rs"]
pub mod envelope;

#[path = "tari.dht.rs"]
pub mod dht;

#[path = "tari.dht.store_forward.rs"]
pub mod store_forward;

#[path = "tari.dht.message_header.rs"]
pub mod message_header;

//---------------------------------- Network impl --------------------------------------------//

impl envelope::Network {
    pub fn is_mainnet(self) -> bool {
        match self {
            Network::MainNet => true,
            _ => false,
        }
    }

    pub fn is_testnet(self) -> bool {
        match self {
            Network::TestNet => true,
            _ => false,
        }
    }

    pub fn is_localtest(self) -> bool {
        match self {
            Network::LocalTest => true,
            _ => false,
        }
    }
}

//---------------------------------- RejectMessage --------------------------------------------//

impl fmt::Display for dht::RejectMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RejectMessage(Reason = {})", self.reason)
    }
}

//---------------------------------- JoinMessage --------------------------------------------//

impl fmt::Display for dht::JoinMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "JoinMessage(NodeId = {}, Addresses = {:?}, Features = {:?})",
            self.node_id.to_hex(),
            self.addresses,
            self.peer_features
        )
    }
}
