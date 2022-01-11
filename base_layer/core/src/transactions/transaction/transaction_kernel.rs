// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use std::{
    cmp::Ordering,
    fmt::{self, Display, Formatter},
};

use blake2::Digest;
use serde::{
    de::{self, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize,
    Serialize,
    Serializer,
};
use tari_common_types::types::{Commitment, HashDigest, Signature};
use tari_crypto::tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray};

use crate::transactions::{
    tari_amount::MicroTari,
    transaction::{KernelFeatures, TransactionError},
    transaction_protocol::{build_challenge, TransactionMetadata},
};

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionKernel {
    pub version: u8,
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: MicroTari,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments (minus an offset). If the transaction is well-formed,
    /// amounts plus fee will sum to zero, and the excess is hence a valid public key.
    pub excess: Commitment,
    /// An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset
    /// excess of the sender.
    pub excess_sig: Signature,
}

impl TransactionKernel {
    pub fn new(
        features: KernelFeatures,
        fee: MicroTari,
        lock_height: u64,
        excess: Commitment,
        excess_sig: Signature,
    ) -> TransactionKernel {
        TransactionKernel {
            version: 0,
            features,
            fee,
            lock_height,
            excess,
            excess_sig,
        }
    }

    pub fn is_coinbase(&self) -> bool {
        self.features.contains(KernelFeatures::COINBASE_KERNEL)
    }

    pub fn verify_signature(&self) -> Result<(), TransactionError> {
        let excess = self.excess.as_public_key();
        let r = self.excess_sig.get_public_nonce();
        let m = TransactionMetadata {
            lock_height: self.lock_height,
            fee: self.fee,
        };
        let c = build_challenge(r, &m);
        if self.excess_sig.verify_challenge(excess, &c) {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying kernel signature".to_string(),
            ))
        }
    }

    /// Produce a canonical hash for a transaction kernel. The hash is given by
    /// $$ H(feature_bits | fee | lock_height | P_excess | R_sum | s_sum)
    pub fn try_hash(&self) -> Result<Vec<u8>, String> {
        match self.version {
            0 => Ok(HashDigest::new()
                .chain(self.version.to_le_bytes())
                .chain(&[self.features.bits()])
                .chain(u64::from(self.fee).to_le_bytes())
                .chain(self.lock_height.to_le_bytes())
                .chain(self.excess.as_bytes())
                .chain(self.excess_sig.get_public_nonce().as_bytes())
                .chain(self.excess_sig.get_signature().as_bytes())
                .finalize()
                .to_vec()),
            _ => Err("new version needs implementing!".to_string()),
        }
    }
}

impl Serialize for TransactionKernel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let mut state = serializer.serialize_struct("TransactionKernel", 6)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("features", &self.features)?;
        state.serialize_field("fee", &self.fee)?;
        state.serialize_field("lock_height", &self.lock_height)?;
        state.serialize_field("excess", &self.excess)?;
        state.serialize_field("excess_sig", &self.excess_sig)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for TransactionKernel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Version,
            Features,
            Fee,
            LockHeight,
            Excess,
            ExcessSig,
        }

        struct TransactionKernelVisitor;

        impl<'de> Visitor<'de> for TransactionKernelVisitor {
            type Value = TransactionKernel;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct TransactionKernel")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<TransactionKernel, V::Error>
            where V: SeqAccess<'de> {
                let version: u8 = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                match version {
                    0 => {
                        let features = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
                        let fee = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
                        let lock_height = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(3, &self))?;
                        let excess = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(4, &self))?;
                        let excess_sig = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(5, &self))?;
                        Ok(TransactionKernel::new(features, fee, lock_height, excess, excess_sig))
                    },
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str("new version needs implementing!"),
                        &self,
                    )),
                }
            }

            fn visit_map<V>(self, mut map: V) -> Result<TransactionKernel, V::Error>
            where V: MapAccess<'de> {
                let mut version = None;
                let mut features = None;
                let mut fee = None;
                let mut lock_height = None;
                let mut excess = None;
                let mut excess_sig = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Version => {
                            if version.is_some() {
                                return Err(de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        },
                        Field::Features => {
                            if features.is_some() {
                                return Err(de::Error::duplicate_field("features"));
                            }
                            features = Some(map.next_value()?);
                        },
                        Field::Fee => {
                            if fee.is_some() {
                                return Err(de::Error::duplicate_field("fee"));
                            }
                            fee = Some(map.next_value()?);
                        },
                        Field::LockHeight => {
                            if lock_height.is_some() {
                                return Err(de::Error::duplicate_field("lock_height"));
                            }
                            lock_height = Some(map.next_value()?);
                        },
                        Field::Excess => {
                            if excess.is_some() {
                                return Err(de::Error::duplicate_field("excess"));
                            }
                            excess = Some(map.next_value()?);
                        },
                        Field::ExcessSig => {
                            if excess_sig.is_some() {
                                return Err(de::Error::duplicate_field("excess_sig"));
                            }
                            excess_sig = Some(map.next_value()?);
                        },
                    }
                }
                let version: u8 = version.ok_or_else(|| de::Error::missing_field("version"))?;
                match version {
                    0 => {
                        let features = features.ok_or_else(|| de::Error::missing_field("features"))?;
                        let fee = fee.ok_or_else(|| de::Error::missing_field("fee"))?;
                        let lock_height = lock_height.ok_or_else(|| de::Error::missing_field("lock_height"))?;
                        let excess = excess.ok_or_else(|| de::Error::missing_field("excess"))?;
                        let excess_sig = excess_sig.ok_or_else(|| de::Error::missing_field("excess_sig"))?;
                        Ok(TransactionKernel::new(features, fee, lock_height, excess, excess_sig))
                    },
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str("new version needs implementing!"),
                        &self,
                    )),
                }
            }
        }

        const FIELDS: &'static [&'static str] = &["version", "features", "fee", "lock_height", "excess", "excess_sig"];
        deserializer.deserialize_struct("TransactionKernel", FIELDS, TransactionKernelVisitor)
    }
}

impl Display for TransactionKernel {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "Fee: {}\nLock height: {}\nFeatures: {:?}\nExcess: {}\nExcess signature: {}\n",
            self.fee,
            self.lock_height,
            self.features,
            self.excess.to_hex(),
            self.excess_sig
                .to_json()
                .unwrap_or_else(|_| "Failed to serialize signature".into()),
        )
    }
}

impl PartialOrd for TransactionKernel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.excess_sig.partial_cmp(&other.excess_sig)
    }
}

impl Ord for TransactionKernel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.excess_sig.cmp(&other.excess_sig)
    }
}
