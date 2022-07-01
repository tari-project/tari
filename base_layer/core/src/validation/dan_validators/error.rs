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

use tari_common_types::types::FixedHash;

use crate::transactions::transaction_components::OutputType;

#[derive(Debug, thiserror::Error)]
pub enum DanLayerValidationError {
    #[error("{output_type} output with contract id {contract_id} is missing required feature data")]
    MissingContractData {
        contract_id: FixedHash,
        output_type: OutputType,
    },
    #[error("Data inconsistency: {details}")]
    DataInconsistency { details: String },
    #[error("Contract constitution not found for contract_id {contract_id}")]
    ContractConstitutionNotFound { contract_id: FixedHash },
    #[error("Sidechain features not provided")]
    SidechainFeaturesNotProvided,
    #[error("Contract update proposal not found for contract_id {contract_id} and proposal_id {proposal_id}")]
    ContractUpdateProposalNotFound { contract_id: FixedHash, proposal_id: u64 },
    #[error("Invalid output type: expected {expected} but got {got}")]
    UnexpectedOutputType { got: OutputType, expected: OutputType },
    #[error("Contract acceptance features not found")]
    ContractAcceptanceNotFound,
    #[error("Duplicate {output_type} contract UTXO: {details}")]
    DuplicateUtxo {
        contract_id: FixedHash,
        output_type: OutputType,
        details: String,
    },
    #[error("Validator node public key is not in committee ({public_key})")]
    ValidatorNotInCommittee { public_key: String },
    #[error("Contract definition not found for contract_id ({contract_id})")]
    ContractDefnintionNotFound { contract_id: FixedHash },
    #[error("Sidechain features data for {field_name} not provided")]
    SideChainFeaturesDataNotProvided { field_name: &'static str },
    #[error("The updated_constitution of the amendment does not match the one in the update proposal")]
    UpdatedConstitutionAmendmentMismatch,
    #[error("Acceptance window has expired for contract_id ({contract_id})")]
    AcceptanceWindowHasExpired { contract_id: FixedHash },
    #[error("Proposal acceptance window has expired for contract_id ({contract_id}) and proposal_id ({proposal_id})")]
    ProposalAcceptanceWindowHasExpired { contract_id: FixedHash, proposal_id: u64 },
    #[error("Checkpoint has non-sequential number. Got: {got}, expected: {expected}")]
    CheckpointNonSequentialNumber { got: u64, expected: u64 },
}
