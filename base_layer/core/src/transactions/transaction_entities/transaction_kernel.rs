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

use crate::{
    crypto::tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray, Hashable},
    transactions::{
        tari_amount::MicroTari,
        transaction_entities::{KernelFeatures, TransactionError},
        transaction_protocol::{build_challenge, TransactionMetadata},
    },
};
use blake2::Digest;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
};
use tari_common_types::types::{Commitment, HashDigest, Signature};

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionKernel {
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

    /// This method was used to sort kernels. It has been replaced, and will be removed in future
    pub fn deprecated_cmp(&self, other: &Self) -> Ordering {
        self.features
            .cmp(&other.features)
            .then(self.fee.cmp(&other.fee))
            .then(self.lock_height.cmp(&other.lock_height))
            .then(self.excess.cmp(&other.excess))
            .then(self.excess_sig.cmp(&other.excess_sig))
    }
}

impl Hashable for TransactionKernel {
    /// Produce a canonical hash for a transaction kernel. The hash is given by
    /// $$ H(feature_bits | fee | lock_height | P_excess | R_sum | s_sum)
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(&[self.features.bits()])
            .chain(u64::from(self.fee).to_le_bytes())
            .chain(self.lock_height.to_le_bytes())
            .chain(self.excess.as_bytes())
            .chain(self.excess_sig.get_public_nonce().as_bytes())
            .chain(self.excess_sig.get_signature().as_bytes())
            .finalize()
            .to_vec()
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
