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

use tari_crypto::{common::Blake256, keys::PublicKey, ristretto::RistrettoPublicKey};

/// The message protocol version for the MessageEnvelopeHeader
pub const MESSAGE_PROTOCOL_VERSION: u8 = 0;

/// The wire protocol version for the MessageEnvelope wire format
pub const WIRE_PROTOCOL_VERSION: u8 = 0;

/// The default port that control services listen on
pub const DEFAULT_LISTENER_ADDRESS: &str = "0.0.0.0:7899";

/// Specify the digest type for the signature challenges
pub type Challenge = Blake256;

/// Public key type
pub type CommsPublicKey = RistrettoPublicKey;
pub type CommsSecretKey = <CommsPublicKey as PublicKey>::K;

/// Message envelop header type
pub type MessageEnvelopeHeader = crate::message::MessageEnvelopeHeader<CommsPublicKey>;
