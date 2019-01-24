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
    range_proof::RangeProof,
    types::{Commitment, Signature},
};

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

bitflags! {
    pub struct OutputFeatures: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT = 0b00000001;
    }
}

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for coinbase output.
    pub features: OutputFeatures,
    /// The commit referencing the output being spent.
    pub commit: Commitment,
}

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Copy, Clone)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commit: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: RangeProof,
}

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
pub struct TransactionKernel {
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: u64,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    pub excess: Commitment,
    /// The signature proving the excess is a valid public key, which signs
    /// the transaction fee.
    pub excess_sig: Signature,
}

pub type BlindingFactor = [u8; 32];
