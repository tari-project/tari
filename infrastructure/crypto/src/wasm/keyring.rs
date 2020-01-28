// Copyright 2020. The Tari Project
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

use wasm_bindgen::prelude::*;

use crate::{
    common::Blake256,
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use blake2::Digest;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tari_utilities::hex::Hex;

#[wasm_bindgen]
pub struct KeyRing {
    keys: HashMap<String, (RistrettoSecretKey, RistrettoPublicKey)>,
}

#[derive(Serialize, Deserialize)]
pub struct SignResult {
    public_nonce: Option<String>,
    signature: Option<String>,
    error: String,
}

#[wasm_bindgen]
impl KeyRing {
    /// Create new keyring
    pub fn new() -> Self {
        KeyRing { keys: HashMap::new() }
    }

    /// Create a new random keypair and associate it with 'id'. The number of keys in the keyring is returned
    pub fn new_key(&mut self, id: String) -> usize {
        let pair = RistrettoPublicKey::random_keypair(&mut OsRng);
        self.keys.insert(id, pair);
        self.keys.len()
    }

    /// Return the number of keys in the keyring
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Return the private key associated with 'id' as a hex string. If there is no key associated with the `id`,
    /// `None` is returned.
    pub fn private_key(&self, id: &str) -> Option<String> {
        self.keys.get(id).and_then(|p| Some(p.0.to_hex()))
    }

    /// Return the public key associated with 'id' as a hex string. If there is no key associated with the `id`,
    /// `None` is returned.
    pub fn public_key(&self, id: &str) -> Option<String> {
        self.keys.get(id).and_then(|p| Some(p.1.to_hex()))
    }

    /// Sign a message using a private key
    ///
    /// Use can use a key in the keyring to generate a digital signature. To create the signature, the caller must
    /// provide the `id` associated with the key, the message to sign, and a `nonce`.
    ///
    /// It is _incredibly important_ to choose the nonce completely randomly, and to never re-use nonces (use
    /// `keys::generate_keypair` for this). If you don't heed this warning, you will give away the private key that
    /// you used to create the signature.
    ///
    /// The return type is pretty unRust-like, but is structured to more closely model a JSON object.
    ///
    /// `keys::check_signature` is used to verify signatures.
    pub fn sign(&self, id: &str, nonce: &str, msg: &str) -> JsValue {
        let mut result = SignResult {
            public_nonce: None,
            signature: None,
            error: "".into(),
        };
        let k = self.keys.get(id);
        if k.is_none() {
            result.error = format!("Private key for '{}' does not exist", id);
            return JsValue::from_serde(&result).unwrap();
        }
        let r = RistrettoSecretKey::from_hex(nonce);
        if r.is_err() {
            result.error = format!("{} is not a valid nonce", nonce);
            return JsValue::from_serde(&result).unwrap();
        }
        let r = r.unwrap();
        let k = k.unwrap();
        let r_pub = RistrettoPublicKey::from_secret_key(&r);
        let e = Blake256::digest(msg.as_bytes());
        let sig = RistrettoSchnorr::sign(k.0.clone(), r, e.as_slice());
        if sig.is_err() {
            result.error = format!("Could not create signature. {}", sig.unwrap_err().to_string());
            return JsValue::from_serde(&result).unwrap();
        }
        let sig = sig.unwrap();
        result.public_nonce = Some(r_pub.to_hex());
        result.signature = Some(sig.get_signature().to_hex());
        JsValue::from_serde(&result).unwrap()
    }
}
