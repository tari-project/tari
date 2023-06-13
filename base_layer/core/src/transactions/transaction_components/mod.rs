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

use chacha20poly1305::Key;
pub use encrypted_data::{EncryptedData, EncryptedDataError};
pub use error::TransactionError;
pub use kernel_builder::KernelBuilder;
pub use kernel_features::KernelFeatures;
pub use kernel_sum::KernelSum;
pub use key_manager_output::KeyManagerOutput;
pub use key_manager_output_builder::KeyManagerOutputBuilder;
pub use output_features::OutputFeatures;
pub use output_features_version::OutputFeaturesVersion;
pub use output_type::OutputType;
pub use range_proof_type::RangeProofType;
pub use side_chain::*;
use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_script::TariScript;
use tari_utilities::{hidden_type, safe_array::SafeArray, Hidden};
pub use transaction::Transaction;
pub use transaction_builder::TransactionBuilder;
pub use transaction_input::{SpentOutput, TransactionInput};
pub use transaction_input_version::TransactionInputVersion;
pub use transaction_kernel::TransactionKernel;
pub use transaction_kernel_version::TransactionKernelVersion;
pub use transaction_output::TransactionOutput;
pub use transaction_output_version::TransactionOutputVersion;
pub use unblinded_output::UnblindedOutput;
use zeroize::Zeroize;

pub mod encrypted_data;
mod error;
mod kernel_builder;
mod kernel_features;
mod kernel_sum;
mod output_features;
mod output_features_version;
mod output_type;
mod range_proof_type;
mod side_chain;

mod key_manager_output;
mod key_manager_output_builder;
mod transaction;
mod transaction_builder;
mod transaction_input;
mod transaction_input_version;
mod transaction_kernel;
mod transaction_kernel_version;
pub mod transaction_output;
mod transaction_output_version;
mod unblinded_output;

#[cfg(test)]
mod test;

// Tx_weight(inputs(12,500), outputs(500), kernels(1)) = 126,510 still well enough below block weight of 127,795
pub const MAX_TRANSACTION_INPUTS: usize = 12_500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 500;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;
pub(crate) const AEAD_KEY_LEN: usize = std::mem::size_of::<Key>();

// Type for hiding aead key encryption
hidden_type!(EncryptedValueKey, SafeArray<u8, AEAD_KEY_LEN>);
hidden_type!(EncryptedDataKey, SafeArray<u8, AEAD_KEY_LEN>);

//----------------------------------------     Crate functions   ----------------------------------------------------//

use super::tari_amount::MicroTari;
use crate::{consensus::DomainSeparatedConsensusHasher, covenants::Covenant, transactions::TransactionHashDomain};

/// Implement the canonical hashing function for TransactionOutput and UnblindedOutput for use in
/// ordering as well as for the output hash calculation for TransactionInput.
///
/// We can exclude the range proof from this hash. The rationale for this is:
/// a) It is a significant performance boost, since the RP is the biggest part of an output
/// b) Range proofs are committed to elsewhere and so we'd be hashing them twice (and as mentioned, this is slow)
pub(super) fn hash_output(
    version: TransactionOutputVersion,
    features: &OutputFeatures,
    commitment: &Commitment,
    script: &TariScript,
    covenant: &Covenant,
    encrypted_data: &EncryptedData,
    sender_offset_public_key: &PublicKey,
    minimum_value_promise: MicroTari,
) -> FixedHash {
    let common_hash = DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("transaction_output")
        .chain(&version)
        .chain(features)
        .chain(commitment)
        .chain(script)
        .chain(covenant)
        .chain(encrypted_data)
        .chain(sender_offset_public_key)
        .chain(&minimum_value_promise);

    match version {
        TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common_hash.finalize().into(),
    }
}
