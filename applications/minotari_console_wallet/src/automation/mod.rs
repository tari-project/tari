// Copyright 2020. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

pub mod commands;
pub mod error;
mod utils;
// removed temporarily add back in when used.
// mod prompt;

use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::TariAddress,
    transaction::TxId,
    types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_core::transactions::{
    key_manager::TariKeyId,
    tari_amount::MicroMinotari,
    transaction_components::{EncryptedData, OutputFeatures},
};
use tari_script::{CheckSigSchnorrSignature, ExecutionStack, TariScript};

// Outputs for self with `FaucetCreatePartyDetails`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step1SessionInfo {
    session_id: String,
    fee_per_gram: MicroMinotari,
    commitment_to_spend: String,
    output_hash: String,
    recipient_address: TariAddress,
}

// Outputs for self with `FaucetCreatePartyDetails`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step2OutputsForSelf {
    alias: String,
    wallet_spend_key_id: TariKeyId,
    script_nonce_key_id: TariKeyId,
    sender_offset_key_id: TariKeyId,
    sender_offset_nonce_key_id: TariKeyId,
}

// Outputs for leader with `FaucetCreatePartyDetails`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step2OutputsForLeader {
    script_input_signature: CheckSigSchnorrSignature,
    wallet_public_spend_key: PublicKey,
    public_script_nonce_key: PublicKey,
    public_sender_offset_key: PublicKey,
    public_sender_offset_nonce_key: PublicKey,
    dh_shared_secret_public_key: PublicKey,
}

// Outputs for self with `FaucetEncumberAggregateUtxo`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step3OutputsForSelf {
    tx_id: TxId,
}

// Outputs for parties with `FaucetEncumberAggregateUtxo`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step3OutputsForParties {
    input_stack: ExecutionStack,
    input_script: TariScript,
    total_script_key: PublicKey,
    script_signature_ephemeral_commitment: Commitment,
    script_signature_ephemeral_pubkey: PublicKey,
    output_commitment: Commitment,
    sender_offset_pubkey: PublicKey,
    metadata_signature_ephemeral_commitment: Commitment,
    metadata_signature_ephemeral_pubkey: PublicKey,
    encrypted_data: EncryptedData,
    output_features: OutputFeatures,
}

// Outputs for leader with `FaucetCreateScriptSig` and `FaucetCreateMetaSig`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step4OutputsForLeader {
    script_signature: Signature,
    metadata_signature: Signature,
    script_offset: PrivateKey,
}
