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
use tari_common_types::types::{Commitment, FixedHash, PublicKey, Signature};
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

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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
    pub excess: Commitment,
    /// An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset
    /// excess of the sender.
    pub excess_sig: Signature,
    /// This is an optional field that must be set if the transaction contains a burned output.
    pub burn_commitment: Option<Commitment>,
}

impl TransactionKernel {
    pub fn new(
        version: TransactionKernelVersion,
        features: KernelFeatures,
        fee: MicroMinotari,
        lock_height: u64,
        excess: Commitment,
        excess_sig: Signature,
        burn_commitment: Option<Commitment>,
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
        excess: Commitment,
        excess_sig: Signature,
        burn_commitment: Option<Commitment>,
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
        let excess = self.excess.as_public_key();
        let r = self.excess_sig.get_public_nonce();
        let c = TransactionKernel::build_kernel_signature_challenge(
            &self.version,
            r,
            excess,
            self.fee,
            self.lock_height,
            &self.features,
            &self.burn_commitment,
        );
        if self.excess_sig.verify_raw_uniform(excess, &c) {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError(
                "Verifying kernel signature".to_string(),
            ))
        }
    }

    /// This gets the burn commitment if it exists
    pub fn get_burn_commitment(&self) -> Result<&Commitment, TransactionError> {
        match self.burn_commitment {
            Some(ref burn_commitment) => Ok(burn_commitment),
            None => Err(TransactionError::InvalidKernel("Burn commitment not found".to_string())),
        }
    }

    /// This is a helper fuction for build kernel challange that does not take in the individual fields,
    /// but rather takes in the TransactionMetadata object.
    pub fn build_kernel_challenge_from_tx_meta(
        version: &TransactionKernelVersion,
        sum_public_nonces: &PublicKey,
        total_excess: &PublicKey,
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
        sum_public_nonces: &PublicKey,
        total_excess: &PublicKey,
        fee: MicroMinotari,
        lock_height: u64,
        features: &KernelFeatures,
        burn_commitment: &Option<Commitment>,
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
        sum_public_nonces: &PublicKey,
        total_excess: &PublicKey,
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
        burn_commitment: &Option<Commitment>,
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
