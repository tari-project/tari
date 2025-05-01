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

use rand::{rngs::OsRng, RngCore};

/// Represents a local peer id. This number is meaningless outside of this node.
pub type PeerId = u64;

/// Generates a random peer key that is guaranteed to be positive '< u64::MAX'.
pub fn generate_peer_key() -> PeerId {
    OsRng.next_u64().saturating_sub(1)
}

/// Generates a random peer key that is guaranteed to be positive '< i64::MAX'.
pub fn generate_peer_id_as_i64() -> i64 {
    i64::try_from(generate_peer_key() % u64::try_from(i64::MAX).expect("infallible")).expect("infallible")
}

/// Converts a positive i64 to a PeerId. This is infallible as the value is guaranteed to be positive.
pub fn peer_id_from_i64(value: i64) -> PeerId {
    let value = if value == i64::MIN { i64::MAX } else { value.abs() };
    u64::try_from(value).expect("infallible")
}
