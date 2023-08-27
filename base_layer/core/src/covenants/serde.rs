//  Copyright 2021, The Taiji Project
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

use std::fmt;

use serde::{
    de::{Error, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};
use tari_utilities::hex::{from_hex, Hex};

use crate::covenants::Covenant;

impl Serialize for Covenant {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let bytes = self.to_bytes();
        if ser.is_human_readable() {
            ser.serialize_str(&bytes.to_hex())
        } else {
            ser.serialize_bytes(&bytes)
        }
    }
}

struct CovenantVisitor;

impl<'de> Visitor<'de> for CovenantVisitor {
    type Value = Covenant;

    fn expecting(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("Expecting a binary array or hex string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where E: Error {
        let bytes = from_hex(v).map_err(|e| E::custom(e.to_string()))?;
        self.visit_bytes(&bytes)
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where E: Error {
        self.visit_str(&v)
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where E: Error {
        let mut v = v;
        Covenant::from_bytes(&mut v).map_err(|e| E::custom(e.to_string()))
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where E: Error {
        self.visit_bytes(v)
    }
}

impl<'de> Deserialize<'de> for Covenant {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        if de.is_human_readable() {
            de.deserialize_string(CovenantVisitor)
        } else {
            de.deserialize_bytes(CovenantVisitor)
        }
    }
}
