//  Copyright 2021. The Tari Project
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

use std::fmt::Display;

use console_error_panic_hook;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_crypto::{common::Blake256, keys::PublicKey as PublicKeyTrait};
use wasm_bindgen::prelude::*;

use crate::{
    cipher_seed::CipherSeed,
    key_manager::{DerivedKey, KeyManager as GenericKeyManager},
};

type KeyDigest = Blake256;

type KeyManager = GenericKeyManager<PrivateKey, KeyDigest>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct DerivedKeypair {
    private_key: PrivateKey,
    public_key: PublicKey,
    key_index: u64,
}

impl From<DerivedKey<PrivateKey>> for DerivedKeypair {
    fn from(derived: DerivedKey<PrivateKey>) -> Self {
        let private_key = derived.k;
        let public_key = PublicKey::from_secret_key(&private_key);
        let key_index = derived.key_index;

        DerivedKeypair {
            private_key,
            public_key,
            key_index,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct KeyManagerResponse {
    key_manager: KeyManager,
    success: bool,
    error: String,
    keypair: Option<DerivedKeypair>,
}

impl KeyManagerResponse {
    fn error(e: impl Display) -> Self {
        Self {
            error: e.to_string(),
            ..Default::default()
        }
    }

    fn success(key_manager: KeyManager, keypair: Option<DerivedKeypair>) -> Self {
        Self {
            key_manager,
            success: true,
            keypair,
            ..Default::default()
        }
    }
}

impl From<KeyManagerResponse> for JsValue {
    fn from(result: KeyManagerResponse) -> Self {
        match JsValue::from_serde(&result) {
            Ok(val) => val,
            Err(_) => JsValue::null(),
        }
    }
}

#[wasm_bindgen]
/// Create a new key manager with the given branch seed if provided
pub fn key_manager_new(branch_seed: Option<String>) -> JsValue {
    let mut key_manager = KeyManager::new();
    if let Some(branch_seed) = branch_seed {
        key_manager.branch_seed = branch_seed;
    }
    KeyManagerResponse::success(key_manager, None).into()
}

#[wasm_bindgen]
/// Create a key manager from parts
pub fn key_manager_from(seed: JsValue, branch_seed: String, primary_key_index: u64) -> JsValue {
    let seed = match parse::<CipherSeed>(&seed) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let key_manager = KeyManager::from(seed, branch_seed, primary_key_index);
    KeyManagerResponse::success(key_manager, None).into()
}

#[wasm_bindgen]
/// Derive the next key
pub fn next_key(key_manager: &JsValue) -> JsValue {
    let mut key_manager = match parse::<KeyManager>(key_manager) {
        Ok(km) => km,
        Err(e) => return e,
    };
    let keypair: DerivedKeypair = match key_manager.next_key() {
        Ok(k) => k.into(),
        Err(e) => return KeyManagerResponse::error(e).into(),
    };

    KeyManagerResponse::success(key_manager, Some(keypair)).into()
}

#[wasm_bindgen]
/// Derive a key for a given index
pub fn derive_key(key_manager: &JsValue, key_index: u64) -> JsValue {
    console_error_panic_hook::set_once();

    let key_manager = match parse::<KeyManager>(key_manager) {
        Ok(km) => km,
        Err(e) => return e,
    };
    let keypair: DerivedKeypair = match key_manager.derive_key(key_index) {
        Ok(k) => k.into(),
        Err(e) => return KeyManagerResponse::error(e).into(),
    };

    KeyManagerResponse::success(key_manager, Some(keypair)).into()
}

/// Parse a T from a JsValue
fn parse<T>(js: &JsValue) -> Result<T, JsValue>
where T: for<'a> Deserialize<'a> {
    match JsValue::into_serde::<T>(js) {
        Ok(t) => Ok(t),
        Err(e) => {
            let msg = format!("Error parsing object: {}", e);
            Err(KeyManagerResponse::error(msg).into())
        },
    }
}

mod test {
    use tari_crypto::tari_utilities::hex::Hex;
    use wasm_bindgen_test::*;

    use super::*;

    #[wasm_bindgen_test]
    fn it_creates_new_key_manager() {
        let js = key_manager_new(None);
        let response = parse::<KeyManagerResponse>(&js).unwrap();

        assert!(response.success);
        assert!(response.keypair.is_none());
        assert_eq!(response.key_manager.branch_seed, "");
    }

    #[wasm_bindgen_test]
    fn it_creates_key_manager_from() {
        let bytes = &[
            0, 119, 156, 172, 30, 41, 29, 120, 191, 26, 160, 11, 200, 249, 193, 163, 245, 33, 159, 148, 127, 31, 238,
            92, 96, 103, 4, 29, 218, 204, 39, 254, 245,
        ];
        let seed = CipherSeed::from_enciphered_bytes(bytes, None).unwrap();
        let seed = JsValue::from_serde(&seed).unwrap();

        let js = key_manager_from(seed, "asdf".into(), 0);
        let mut response = parse::<KeyManagerResponse>(&js).unwrap();

        assert_eq!(response.key_manager.branch_seed, "asdf");
        let next_key = response.key_manager.next_key().unwrap();
        assert_eq!(
            next_key.k.to_hex(),
            "5a14f1205cfeb10d53af46e82b70e8832a544206f524b404e7a346148532910a".to_string()
        )
    }

    #[wasm_bindgen_test]
    fn it_derives_keys() {
        let js = key_manager_new(None);
        let response = parse::<KeyManagerResponse>(&js).unwrap();
        assert_eq!(response.key_manager.key_index(), 0);

        let km = JsValue::from_serde(&response.key_manager).unwrap();
        let response = next_key(&km);
        let response = parse::<KeyManagerResponse>(&response).unwrap();
        let keypair1 = response.keypair.clone().unwrap();

        assert!(response.success);
        assert!(response.keypair.is_some());
        assert_eq!(response.key_manager.key_index(), 1);

        let response = derive_key(&km, 1);
        let response = parse::<KeyManagerResponse>(&response).unwrap();
        let keypair2 = response.keypair.unwrap();

        assert_eq!(keypair1, keypair2);
    }
}
