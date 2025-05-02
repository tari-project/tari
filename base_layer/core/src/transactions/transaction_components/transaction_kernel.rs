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
    fmt::{Display, Formatter},
};

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::consts::{U32, U64};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedCommitment, CompressedPublicKey, FixedHash, Signature};
use tari_hashing::TransactionHashDomain;
use tari_utilities::{hex::Hex, message_format::MessageFormat};

use super::TransactionKernelVersion;
use crate::{
    consensus::DomainSeparatedConsensusHasher,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{KernelFeatures, TransactionError},
        transaction_protocol::TransactionMetadata,
    },
};

#[cfg(test)]
pub const MAX_KERNEL_SIZE: usize = 132; // 127 (max size from unit test) + 5 (margin)

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default)]
pub struct TransactionKernel {
    pub version: TransactionKernelVersion,
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: MicroMinotari,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments (minus an offset). If the transaction is well-formed,
    /// amounts plus fee will sum to zero, and the excess is hence a valid public key.
    pub excess: CompressedCommitment,
    /// An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset
    /// excess of the sender.
    pub excess_sig: Signature,
    /// This is an optional field that must be set if the transaction contains a burned output.
    pub burn_commitment: Option<CompressedCommitment>,
}

impl TransactionKernel {
    pub fn new(
        version: TransactionKernelVersion,
        features: KernelFeatures,
        fee: MicroMinotari,
        lock_height: u64,
        excess: CompressedCommitment,
        excess_sig: Signature,
        burn_commitment: Option<CompressedCommitment>,
    ) -> TransactionKernel {
        TransactionKernel {
            version,
            features,
            fee,
            lock_height,
            excess,
            excess_sig,
            burn_commitment,
        }
    }

    /// Produce a canonical hash for a transaction kernel.
    pub fn hash(&self) -> FixedHash {
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("transaction_kernel")
            .chain(self)
            .finalize()
            .into()
    }

    pub fn new_current_version(
        features: KernelFeatures,
        fee: MicroMinotari,
        lock_height: u64,
        excess: CompressedCommitment,
        excess_sig: Signature,
        burn_commitment: Option<CompressedCommitment>,
    ) -> TransactionKernel {
        TransactionKernel::new(
            TransactionKernelVersion::get_current_version(),
            features,
            fee,
            lock_height,
            excess,
            excess_sig,
            burn_commitment,
        )
    }

    pub fn is_coinbase(&self) -> bool {
        self.features.is_coinbase()
    }

    /// Is this a burned output kernel?
    pub fn is_burned(&self) -> bool {
        self.features.is_burned()
    }

    pub fn verify_signature(&self) -> Result<(), TransactionError> {
        let excess = self.excess.to_compressed_key();
        let r = self.excess_sig.get_compressed_public_nonce();
        let c = TransactionKernel::build_kernel_signature_challenge(
            &self.version,
            r,
            &excess,
            self.fee,
            self.lock_height,
            &self.features,
            &self.burn_commitment,
        );

        if self
            .excess_sig
            .to_schnorr_signature()?
            .verify_raw_uniform(&excess.to_public_key()?, &c)
        {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying kernel signature".to_string(),
            ))
        }
    }

    /// This gets the burn commitment if it exists
    pub fn get_burn_commitment(&self) -> Result<&CompressedCommitment, TransactionError> {
        match self.burn_commitment {
            Some(ref burn_commitment) => Ok(burn_commitment),
            None => Err(TransactionError::InvalidKernel("Burn commitment not found".to_string())),
        }
    }

    /// This is a helper fuction for build kernel challange that does not take in the individual fields,
    /// but rather takes in the TransactionMetadata object.
    pub fn build_kernel_challenge_from_tx_meta(
        version: &TransactionKernelVersion,
        sum_public_nonces: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        tx_meta: &TransactionMetadata,
    ) -> [u8; 64] {
        TransactionKernel::build_kernel_signature_challenge(
            version,
            sum_public_nonces,
            total_excess,
            tx_meta.fee,
            tx_meta.lock_height,
            &tx_meta.kernel_features,
            &tx_meta.burn_commitment,
        )
    }

    /// Helper function to creates the kernel excess signature challenge.
    /// The challenge is defined as the hash of the following data:
    ///  Public nonce
    ///  Fee
    ///  Lock height
    ///  Features of the kernel
    ///  Burn commitment if present
    pub fn build_kernel_signature_challenge(
        version: &TransactionKernelVersion,
        sum_public_nonces: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        fee: MicroMinotari,
        lock_height: u64,
        features: &KernelFeatures,
        burn_commitment: &Option<CompressedCommitment>,
    ) -> [u8; 64] {
        // We build the message separately to help with hardware wallet support. This reduces the amount of data that
        // needs to be transferred in order to sign the signature.
        let message =
            TransactionKernel::build_kernel_signature_message(version, fee, lock_height, features, burn_commitment);
        TransactionKernel::finalize_kernel_signature_challenge(version, sum_public_nonces, total_excess, &message)
    }

    /// Helper function to finalize the kernel excess signature challenge.
    pub fn finalize_kernel_signature_challenge(
        version: &TransactionKernelVersion,
        sum_public_nonces: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        message: &[u8; 32],
    ) -> [u8; 64] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("kernel_signature")
            .chain(sum_public_nonces)
            .chain(total_excess)
            .chain(message);
        match version {
            TransactionKernelVersion::V0 => common.finalize().into(),
        }
    }

    /// Convenience function to create the entire kernel signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn build_kernel_signature_message(
        version: &TransactionKernelVersion,
        fee: MicroMinotari,
        lock_height: u64,
        features: &KernelFeatures,
        burn_commitment: &Option<CompressedCommitment>,
    ) -> [u8; 32] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("kernel_message")
            .chain(version)
            .chain(&fee)
            .chain(&lock_height)
            .chain(features)
            .chain(burn_commitment);
        match version {
            TransactionKernelVersion::V0 => common.finalize().into(),
        }
    }
}

impl Display for TransactionKernel {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "Fee: {}\nLock height: {}\nFeatures: {:?}\nExcess: {}\nExcess signature: {}\nCommitment: {}\n",
            self.fee,
            self.lock_height,
            self.features,
            self.excess.to_hex(),
            self.excess_sig
                .to_json()
                .unwrap_or_else(|_| "Failed to serialize signature".into()),
            match self.burn_commitment {
                Some(ref burn_commitment) => burn_commitment.to_hex(),
                None => "None".to_string(),
            }
        )
    }
}

impl PartialOrd for TransactionKernel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TransactionKernel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.excess_sig.cmp(&other.excess_sig)
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::{CompressedCommitment, CompressedPublicKey, PrivateKey, Signature};
    use tari_utilities::hex::Hex;

    use crate::{
        borsh::SerializedSize,
        transactions::transaction_components::{transaction_kernel::MAX_KERNEL_SIZE, KernelBuilder},
    };

    #[test]
    fn verify_max_size_const() {
        let s = PrivateKey::from_hex("6c6eebc5a9c02e1f3c16a69ba4331f9f63d0718401dea10adc4f9d3b879a2c09").unwrap();
        let r =
            CompressedPublicKey::from_hex("28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d43b").unwrap();
        let sig = Signature::new(r, s);
        let excess =
            CompressedCommitment::from_hex("9017be5092b85856ce71061cadeb20c2d1fabdf664c4b3f082bf44cf5065e650").unwrap();
        let tx_kernel = KernelBuilder::new()
            .with_signature(sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();

        let tx_kernel_size = tx_kernel.get_serialized_size().unwrap();

        // tx_kernel_size: 127
        // println!("tx_kernel_size: {}", tx_kernel_size);

        assert!(MAX_KERNEL_SIZE >= tx_kernel_size);
    }
}
