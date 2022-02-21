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

pub use asset_output_features::AssetOutputFeatures;
use blake2::Digest;
pub use committee_definition_features::CommitteeDefinitionFeatures;
pub use error::TransactionError;
pub use full_rewind_result::FullRewindResult;
pub use kernel_builder::KernelBuilder;
pub use kernel_features::KernelFeatures;
pub use kernel_sum::KernelSum;
pub use mint_non_fungible_features::MintNonFungibleFeatures;
pub use output_features::OutputFeatures;
pub use output_features_version::OutputFeaturesVersion;
pub use output_flags::OutputFlags;
pub use rewind_result::RewindResult;
pub use side_chain_checkpoint_features::SideChainCheckpointFeatures;
use tari_common_types::types::{Commitment, HashDigest};
use tari_crypto::script::TariScript;
pub use template_parameter::TemplateParameter;
pub use transaction::Transaction;
pub use transaction_builder::TransactionBuilder;
pub use transaction_input::{SpentOutput, TransactionInput};
pub use transaction_input_version::TransactionInputVersion;
pub use transaction_kernel::TransactionKernel;
pub use transaction_kernel_version::TransactionKernelVersion;
pub use transaction_output::TransactionOutput;
pub use transaction_output_version::TransactionOutputVersion;
pub use unblinded_output::UnblindedOutput;
pub use unblinded_output_builder::UnblindedOutputBuilder;

mod asset_output_features;
mod committee_definition_features;
mod error;
mod full_rewind_result;
mod kernel_builder;
mod kernel_features;
mod kernel_sum;
mod mint_non_fungible_features;
mod output_features;
mod output_features_version;
mod output_flags;
mod rewind_result;
mod side_chain_checkpoint_features;
mod template_parameter;
mod transaction;
mod transaction_builder;
mod transaction_input;
mod transaction_input_version;
mod transaction_kernel;
mod transaction_kernel_version;
mod transaction_output;
mod transaction_output_version;
mod unblinded_output;
mod unblinded_output_builder;

#[cfg(test)]
mod test;

// Tx_weight(inputs(12,500), outputs(500), kernels(1)) = 126,510 still well enough below block weight of 127,795
pub const MAX_TRANSACTION_INPUTS: usize = 12_500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 500;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;

//----------------------------------------     Crate functions   ----------------------------------------------------//

use crate::{common::hash_writer::HashWriter, consensus::ConsensusEncoding, covenants::Covenant};

/// Implement the canonical hashing function for TransactionOutput and UnblindedOutput for use in
/// ordering as well as for the output hash calculation for TransactionInput.
///
/// We can exclude the range proof from this hash. The rationale for this is:
/// a) It is a significant performance boost, since the RP is the biggest part of an output
/// b) Range proofs are committed to elsewhere and so we'd be hashing them twice (and as mentioned, this is slow)
/// c) TransactionInputs will now have the same hash as UTXOs, which makes locating STXOs easier when doing reorgs
pub(super) fn hash_output(
    version: TransactionOutputVersion,
    features: &OutputFeatures,
    commitment: &Commitment,
    script: &TariScript,
    covenant: &Covenant,
) -> [u8; 32] {
    let mut hasher = HashWriter::new(HashDigest::new());
    // unwrap: hashwriter is infallible
    version.consensus_encode(&mut hasher).unwrap();
    features.consensus_encode(&mut hasher).unwrap();
    commitment.consensus_encode(&mut hasher).unwrap();
    script.consensus_encode(&mut hasher).unwrap();
    covenant.consensus_encode(&mut hasher).unwrap();
    hasher.finalize()
}
