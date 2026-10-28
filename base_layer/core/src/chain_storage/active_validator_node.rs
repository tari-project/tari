//  Copyright 2022, The Tari Project
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

use std::cmp;

use serde::{Deserialize, Deserializer, Serialize, de::Error};
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedCommitment, CompressedPublicKey},
};
use tari_transaction_components::{MicroMinotari, transaction_components::ValidatorNodeRegistration};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidatorNodeEntry {
    pub shard_key: [u8; 32],
    /// The epoch in which this validator node was (or will be) activated
    pub activation_epoch: VnEpoch,
    /// The epoch in which the validator registration UTXO was submitted
    pub registration_epoch: VnEpoch,
    pub public_key: CompressedPublicKey,
    pub commitment: CompressedCommitment,
    pub sidechain_public_key: Option<CompressedPublicKey>,
    pub minimum_value_promise: MicroMinotari,
    /// The registration carried by the output that created this entry
    #[serde(deserialize_with = "deserialize_original_registration")]
    pub original_registration: ValidatorNodeRegistration,
}

/// Entries stored by older versions end before this field, so they fail to decode here instead of being misread.
fn deserialize_original_registration<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ValidatorNodeRegistration, D::Error> {
    ValidatorNodeRegistration::deserialize(deserializer).map_err(|e| {
        D::Error::custom(format!(
            "validator node entry has no original_registration; it was stored by an older version and the chain \
             database must be resynced: {e}"
        ))
    })
}

impl Ord for ValidatorNodeEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sidechain_public_key
            .cmp(&other.sidechain_public_key)
            .then_with(|| self.shard_key.cmp(&other.shard_key))
    }
}

impl PartialOrd for ValidatorNodeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ValidatorNodeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.sidechain_public_key == other.sidechain_public_key && self.shard_key == other.shard_key
    }
}

impl Eq for ValidatorNodeEntry {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_without_original_registration_fails_to_decode() {
        let entry = ValidatorNodeEntry::default();
        let bytes = bincode::serialize(&entry).unwrap();
        let registration_len = bincode::serialized_size(&entry.original_registration).unwrap();
        let old_len = bytes
            .len()
            .checked_sub(usize::try_from(registration_len).unwrap())
            .unwrap();
        let err = bincode::deserialize::<ValidatorNodeEntry>(bytes.get(..old_len).unwrap()).unwrap_err();
        assert!(err.to_string().contains("must be resynced"), "{err}");
        assert!(bincode::deserialize::<ValidatorNodeEntry>(&bytes).is_ok());
    }
}
