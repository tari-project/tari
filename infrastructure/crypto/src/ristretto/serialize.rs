// Copyright 2019. The Tari Project
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

//! Custom serializers for Ristretto keys
//!
//! The Dalek libraries only serialize to binary (understandably), but this has 2 yucky implications:
//!
//! 1. Exporting to "human readable" formats like JSON yield crappy looking 'binary arrays', e.g. /[12, 223, 65, .../]
//! 2. Reading back from JSON is broken because serde doesn't read this back as a byte string, but as a seq.
//!
//! The workaround is to have binary serialization by default, but if a struct is going to be saved in JSON format,
//! then you can override that behaviour with `with_serialize`, e.g.
//!
//! ```nocompile
//!   #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
//!   pub struct KeyManager<K: SecretKey, D: Digest> {
//!       #[serde(serialize_with = "serialise_to_hex", deserialize_with = "secret_from_hex")]
//!       pub master_key: K,
//!       pub branch_seed: String,
//!       pub primary_key_index: usize,
//!       digest_type: PhantomData<D>,
//!   }
//! ```

use crate::keys::{PublicKey, SecretKey};
use serde::{de, Deserializer};
use std::{fmt, marker::PhantomData};
use tari_utilities::hex::Hex;

pub fn secret_from_hex<'de, D, K>(des: D) -> Result<K, D::Error>
where
    D: Deserializer<'de>,
    K: Hex + SecretKey,
{
    struct KeyStringVisitor<K> {
        marker: PhantomData<K>,
    };

    impl<'de, K: SecretKey> de::Visitor<'de> for KeyStringVisitor<K> {
        type Value = K;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a secret key in hex format")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where E: de::Error {
            K::from_hex(v).map_err(E::custom)
        }
    }
    des.deserialize_str(KeyStringVisitor { marker: PhantomData })
}

pub fn pubkey_from_hex<'de, D, K>(des: D) -> Result<K, D::Error>
where
    D: Deserializer<'de>,
    K: Hex + PublicKey,
{
    struct KeyStringVisitor<K> {
        marker: PhantomData<K>,
    };

    impl<'de, K: PublicKey> de::Visitor<'de> for KeyStringVisitor<K> {
        type Value = K;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a public key in hex format")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where E: de::Error {
            K::from_hex(v).map_err(E::custom)
        }
    }
    des.deserialize_str(KeyStringVisitor { marker: PhantomData })
}
