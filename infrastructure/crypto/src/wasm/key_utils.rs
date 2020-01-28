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

//! Simple cryptographic key functions. It's generally not very efficient to use these functions to do lots of cool
//! stuff with private and public keys, because the keys are transalted to- and from hex every time you make a call
//! using a function from this module. You should use a [KeyRing] instead. But sometimes, these functions are handy.

use crate::{
    common::Blake256,
    keys::PublicKey,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
};
use blake2::Digest;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_utilities::hex::Hex;
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize)]
pub struct SignatureVerifyResult {
    result: bool,
    error: String,
}

/// Create an return a new private- public key pair
#[wasm_bindgen]
pub fn generate_keypair() -> JsValue {
    let (k, p) = RistrettoPublicKey::random_keypair(&mut OsRng);
    let pair = (k.to_hex(), p.to_hex());
    JsValue::from_serde(&pair).unwrap()
}

/// Calculate the public key associated with a private key. If the input is not a valid hex string representing a
/// private key, `None` is returned
#[wasm_bindgen]
pub fn pubkey_from_secret(k: &str) -> Option<String> {
    match RistrettoSecretKey::from_hex(k) {
        Ok(k) => Some(RistrettoPublicKey::from_secret_key(&k).to_hex()),
        _ => None,
    }
}

/// Checks
#[allow(non_snake_case)]
#[wasm_bindgen]
pub fn check_signature(pub_nonce: &str, signature: &str, pub_key: &str, msg: &str) -> JsValue {
    let mut result = SignatureVerifyResult {
        result: false,
        error: "".into(),
    };

    let R = RistrettoPublicKey::from_hex(pub_nonce);
    if R.is_err() {
        result.error = format!("{} is not a valid public nonce", pub_nonce);
        return JsValue::from_serde(&result).unwrap();
    }
    let R = R.unwrap();

    let P = RistrettoPublicKey::from_hex(pub_key);
    if P.is_err() {
        result.error = format!("{} is not a valid public key", pub_key);
        return JsValue::from_serde(&result).unwrap();
    }
    let P = P.unwrap();

    let s = RistrettoSecretKey::from_hex(signature);
    if s.is_err() {
        result.error = format!("{} is not a valid hex representation of a signature", signature);
        return JsValue::from_serde(&result).unwrap();
    }
    let s = s.unwrap();

    let sig = RistrettoSchnorr::new(R, s);
    let msg = Blake256::digest(msg.as_bytes());
    result.result = sig.verify_challenge(&P, msg.as_slice());
    JsValue::from_serde(&result).unwrap()
}
