//  Copyright 2022. The Tari Project
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

mod contract_acceptance;
pub use contract_acceptance::ContractAcceptance;

mod contract_acceptance_challenge;
pub use contract_acceptance_challenge::ContractAcceptanceChallenge;

mod contract_constitution;
pub use contract_constitution::{
    CheckpointParameters,
    ConstitutionChangeFlags,
    ConstitutionChangeRules,
    ContractAcceptanceRequirements,
    ContractConstitution,
    RequirementsForConstitutionChange,
    SideChainConsensus,
};

mod contract_definition;
pub use contract_definition::{ContractDefinition, ContractSpecification, FunctionRef, PublicFunction};

mod contract_update_proposal;
pub use contract_update_proposal::ContractUpdateProposal;

mod contract_update_proposal_acceptance;
pub use contract_update_proposal_acceptance::ContractUpdateProposalAcceptance;

mod contract_amendment;
pub use contract_amendment::ContractAmendment;

mod committee_members;
pub use committee_members::CommitteeMembers;

mod committee_signatures;
pub use committee_signatures::CommitteeSignatures;

mod signer_signature;
pub use signer_signature::SignerSignature;

mod sidechain_features;
pub use sidechain_features::{SideChainFeatures, SideChainFeaturesBuilder};

mod contract_checkpoint;
pub use contract_checkpoint::ContractCheckpoint;

mod checkpoint_challenge;
pub use checkpoint_challenge::CheckpointChallenge;

// Length of FixedString
pub const FIXED_STR_LEN: usize = 32;
pub type FixedString = [u8; FIXED_STR_LEN];

pub fn bytes_into_fixed_string<T: AsRef<[u8]>>(value: T) -> FixedString {
    tari_common_types::array::copy_into_fixed_array_lossy::<_, FIXED_STR_LEN>(value.as_ref())
}
