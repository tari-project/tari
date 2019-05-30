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

use crate::connection::{ConnectionError, Result};
use clear_on_drop::clear::Clear;
use zmq;

/// Generates a Curve25519 public/private keypair
pub fn generate() -> Result<(CurveSecretKey, CurvePublicKey)> {
    let keypair = zmq::CurveKeyPair::new()
        .map_err(|e| ConnectionError::CurveKeypairError(format!("Unable to generate new Curve25519 keypair: {}", e)))?;

    return Ok((CurveSecretKey(keypair.secret_key), CurvePublicKey(keypair.public_key)));
}

//---------------------------------- Curve Encryption --------------------------------------------//

/// Represents settings for asymmetric curve encryption. Every socket with encryption enabled
/// must either act as a server or client.
#[derive(Clone)]
pub enum CurveEncryption {
    /// No encryption
    None,
    /// Act as a server which accepts all connections which have a public key corresponding to the
    /// given secret key.
    Server { secret_key: CurveSecretKey },
    /// Act as a client which connects to a server with a given server public key and a client keypair.
    Client {
        secret_key: CurveSecretKey,
        public_key: CurvePublicKey,
        server_public_key: CurvePublicKey,
    },
}

impl Default for CurveEncryption {
    fn default() -> Self {
        CurveEncryption::None
    }
}

//---------------------------------- Curve Secret Key --------------------------------------------//

/// Represents a Curve25519 secret key
#[derive(Clone)]
pub struct CurveSecretKey(pub(crate) [u8; 32]);

impl CurveSecretKey {
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0
    }
}

impl Default for CurveSecretKey {
    fn default() -> Self {
        Self([0u8; 32])
    }
}

impl Drop for CurveSecretKey {
    fn drop(&mut self) {
        self.0.clear();
    }
}

//---------------------------------- Curve Public Key --------------------------------------------//
#[derive(Clone)]
/// Represents a Curve25519 public key
pub struct CurvePublicKey(pub(crate) [u8; 32]);

impl CurvePublicKey {
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0
    }
}

impl Default for CurvePublicKey {
    fn default() -> Self {
        Self([0u8; 32])
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    // Optimisations can cause this test to erroneously fail in release mode. The value is zeroed out on drop though.
    #[cfg(debug_assertions)]
    fn clears_secret_key_on_drop() {
        use std::slice;
        let ptr;
        {
            let sk = generate().unwrap().0;
            ptr = sk.0.as_ptr()
        }

        let zero = &[0u8; 32];
        unsafe {
            assert_eq!(zero, slice::from_raw_parts(ptr, 32));
        }
    }

    #[test]
    fn default_is_zero() {
        assert!(CurveSecretKey::default().is_zero());
        assert!(CurvePublicKey::default().is_zero());
    }
}
