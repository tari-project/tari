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
    types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, PrivateKey},
};
use tari_script::{CompressedCheckSigSchnorrSignature, ExecutionStack, TariScript};
use tari_transaction_components::{
    MicroMinotari,
    key_manager::TariKeyId,
    transaction_components::{EncryptedData, OutputFeatures},
};

// Step 1 outputs for all with `PreMineSpendSessionInfo`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep1SessionInfo {
    session_id: String,
    fee_per_gram: MicroMinotari,
    recipient_info: Vec<RecipientInfo>,
    use_pre_mine_input_file: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct RecipientInfo {
    output_to_be_spend: usize,
    recipient_address: TariAddress,
}

impl SessionId for PreMineSpendStep1SessionInfo {
    fn session_id(&self) -> String {
        self.session_id.clone()
    }
}

// Step 2 outputs for self with `PreMineSpendPartyDetails`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep2OutputsForSelf {
    outputs_for_self: Vec<Step2OutputsForSelf>,
    alias: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step2OutputsForSelf {
    output_index: usize,
    recipient_address: TariAddress,
    script_nonce_key_id: TariKeyId,
    sender_offset_key_id: TariKeyId,
    sender_offset_nonce_key_id: TariKeyId,
    pre_mine_script_key_id: TariKeyId,
    /// `pre_mine_script_key - sender_offset_key`, computed when the key manager generated the sender offset key.
    script_offset: PrivateKey,
}

impl Step2OutputsForSelf {
    /// Check that the key ids this output's step 2 self file names are ones step 2 could have written.
    ///
    /// The self file is host-writable, and step 4 signs with what it names through the legacy nonce instruction, so
    /// a file that names one nonce twice, or a sender offset key where the script key belongs, turns step 4 into a
    /// nonce reuse with no further help. Each id is held to the shape step 2 gives it:
    ///
    /// - the two nonces are different keys;
    /// - each nonce is a ledger `Random` key, or an `Encrypted` key on a software wallet;
    /// - the pre-mine script key is a ledger `PreMine` key without the pre-mine sender offset marker (its genesis
    ///   output index);
    /// - the sender offset key is a ledger `PreMine` key *with* the marker, as `get_script_offset` issues it in
    ///   pre-mine mode, or an `Encrypted` key on a software wallet.
    pub(crate) fn check_key_ids(&self) -> Result<(), String> {
        use minotari_ledger_wallet_common::{
            common_types::LedgerKeyBranch,
            script_offset::is_pre_mine_sender_offset_index,
        };

        let output = self.output_index;
        if self.script_nonce_key_id == self.sender_offset_nonce_key_id {
            return Err(format!(
                "output {output}: the script nonce and the sender offset nonce are the same key ({})",
                self.script_nonce_key_id
            ));
        }
        for (what, nonce) in [
            ("script nonce", &self.script_nonce_key_id),
            ("sender offset nonce", &self.sender_offset_nonce_key_id),
        ] {
            match nonce {
                TariKeyId::LedgerKey {
                    branch: LedgerKeyBranch::Random,
                    ..
                } |
                TariKeyId::Encrypted { .. } => {},
                other => {
                    return Err(format!(
                        "output {output}: the {what} is not a Random nonce key ({other})"
                    ));
                },
            }
        }
        match &self.pre_mine_script_key_id {
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::PreMine,
                index,
            } if !is_pre_mine_sender_offset_index(*index) => {},
            other => {
                return Err(format!(
                    "output {output}: the pre-mine script key is not a pre-mine script key index ({other})"
                ));
            },
        }
        match &self.sender_offset_key_id {
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::PreMine,
                index,
            } if is_pre_mine_sender_offset_index(*index) => {},
            TariKeyId::Encrypted { .. } => {},
            other => {
                return Err(format!(
                    "output {output}: the sender offset key is not a pre-mine sender offset key ({other})"
                ));
            },
        }
        Ok(())
    }
}

// Step 2 outputs for leader with `PreMineSpendPartyDetails`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep2OutputsForLeader {
    outputs_for_leader: Vec<Step2OutputsForLeader>,
    alias: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step2OutputsForLeader {
    output_index: usize,
    recipient_address: TariAddress,
    script_input_signature: CompressedCheckSigSchnorrSignature,
    public_script_nonce_key: CompressedPublicKey,
    public_sender_offset_key: CompressedPublicKey,
    public_sender_offset_nonce_key: CompressedPublicKey,
    dh_shared_secret_public_key: CompressedPublicKey,
    pre_mine_public_script_key: CompressedPublicKey,
}

// Step 3 outputs for self with `PreMineSpendEncumberAggregateUtxo`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep3OutputsForSelf {
    outputs_for_self: Vec<Step3OutputsForSelf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step3OutputsForSelf {
    output_index: usize,
    tx_id: TxId,
}

// Step 3 outputs for parties with `PreMineSpendEncumberAggregateUtxo`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep3OutputsForParties {
    outputs_for_parties: Vec<Step3OutputsForParties>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step3OutputsForParties {
    output_index: usize,
    input_stack: ExecutionStack,
    input_script: TariScript,
    total_script_key: CompressedPublicKey,
    script_signature_ephemeral_commitment: CompressedCommitment,
    script_signature_ephemeral_pubkey: CompressedPublicKey,
    output_commitment: CompressedCommitment,
    sender_offset_pubkey: CompressedPublicKey,
    metadata_signature_ephemeral_commitment: CompressedCommitment,
    metadata_signature_ephemeral_pubkey: CompressedPublicKey,
    encrypted_data: EncryptedData,
    output_features: OutputFeatures,
    shared_secret: CompressedPublicKey,
}

// Step 4 outputs for leader with `PreMineSpendInputOutputSigs`
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct PreMineSpendStep4OutputsForLeader {
    outputs_for_leader: Vec<Step4OutputsForLeader>,
    alias: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Step4OutputsForLeader {
    output_index: usize,
    script_signature: CompressedSignature,
    metadata_signature: CompressedSignature,
    script_offset: PrivateKey,
}

trait SessionId {
    fn session_id(&self) -> String;
}

#[cfg(test)]
mod test {
    use minotari_ledger_wallet_common::{
        common_types::LedgerKeyBranch,
        script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT,
    };
    use tari_transaction_components::key_manager::{KeyManager, TransactionKeyManagerInterface};

    use super::*;

    fn ledger(branch: LedgerKeyBranch, index: u64) -> TariKeyId {
        TariKeyId::LedgerKey { branch, index }
    }

    fn ledger_output() -> Step2OutputsForSelf {
        Step2OutputsForSelf {
            output_index: 3,
            script_nonce_key_id: ledger(LedgerKeyBranch::Random, (1 << 32) | 1),
            sender_offset_key_id: ledger(LedgerKeyBranch::PreMine, PRE_MINE_SENDER_OFFSET_INDEX_BIT | 5),
            sender_offset_nonce_key_id: ledger(LedgerKeyBranch::Random, (1 << 32) | 2),
            pre_mine_script_key_id: ledger(LedgerKeyBranch::PreMine, 3),
            ..Default::default()
        }
    }

    #[test]
    fn what_step_2_writes_passes() {
        assert_eq!(ledger_output().check_key_ids(), Ok(()));

        // A software wallet's nonces and sender offset key are encrypted keys.
        let key_manager = KeyManager::new_random().unwrap();
        let mut software = ledger_output();
        software.script_nonce_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        software.sender_offset_nonce_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        software.sender_offset_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        assert_eq!(software.check_key_ids(), Ok(()));
    }

    #[test]
    fn a_tampered_self_file_is_refused() {
        let mut same_nonce = ledger_output();
        same_nonce.sender_offset_nonce_key_id = same_nonce.script_nonce_key_id.clone();

        let mut nonce_off_random = ledger_output();
        nonce_off_random.script_nonce_key_id = ledger(LedgerKeyBranch::PreMine, (1 << 32) | 1);

        let mut marked_script_key = ledger_output();
        marked_script_key.pre_mine_script_key_id =
            ledger(LedgerKeyBranch::PreMine, PRE_MINE_SENDER_OFFSET_INDEX_BIT | 3);

        let mut unmarked_sender_offset = ledger_output();
        unmarked_sender_offset.sender_offset_key_id = ledger(LedgerKeyBranch::PreMine, 3);

        let mut one_sided_sender_offset = ledger_output();
        one_sided_sender_offset.sender_offset_key_id = ledger(LedgerKeyBranch::OneSidedSenderOffset, 9);

        for (what, output) in [
            ("one nonce twice", same_nonce),
            ("a nonce off the Random branch", nonce_off_random),
            ("a marked script key", marked_script_key),
            ("an unmarked sender offset key", unmarked_sender_offset),
            ("a OneSidedSenderOffset sender offset key", one_sided_sender_offset),
        ] {
            assert!(output.check_key_ids().is_err(), "{what} was accepted");
        }
    }
}
