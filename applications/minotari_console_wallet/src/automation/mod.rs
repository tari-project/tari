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
    /// - the pre-mine script key is the ledger `PreMine` key at this output's genesis index (`output_index`), which
    ///   also keeps it clear of the pre-mine sender offset marker;
    /// - the sender offset key is a ledger `PreMine` key *with* the marker, as `get_script_offset` issues it in
    ///   pre-mine mode, or an `Encrypted` key on a software wallet.
    ///
    /// These are per-output checks; [`check_self_file_key_ids`] adds the ones across outputs, and is what step 4 calls.
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
            } if !is_pre_mine_sender_offset_index(*index) && u64::try_from(output).ok() == Some(*index) => {},
            other => {
                return Err(format!(
                    "output {output}: the pre-mine script key is not the pre-mine key at this output's index ({other})"
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

/// Check a whole step 2 self file before step 4 signs with it: every output on its own
/// ([`Step2OutputsForSelf::check_key_ids`]), and then across outputs - every nonce, script or sender offset, appears
/// once in the whole file, and so does every sender offset key.
///
/// Per-output checks alone let a file swap nonces between outputs: output A on `(n1, n2)` and output B on `(n2, n1)`
/// passes each check, and one honest step 4 run then signs two keys under `n1` and two under `n2`, which with the
/// script offsets in the same file solves for both pre-mine script keys.
///
/// What this enforces is that the file has the shape step 2 writes. It does not stop a leader from having step 4 re-run
/// against a second step 3 file, or a compromised host from signing whatever it likes; see
/// `minotari_ledger_wallet_common::legacy_nonce`.
pub(crate) fn check_self_file_key_ids(outputs: &[Step2OutputsForSelf]) -> Result<(), String> {
    use std::collections::HashSet;

    let mut nonces = HashSet::new();
    let mut sender_offsets = HashSet::new();
    for output in outputs {
        output.check_key_ids()?;
        for nonce in [&output.script_nonce_key_id, &output.sender_offset_nonce_key_id] {
            // `TariKeyId` is not `Hash`; its string form is its canonical encoding.
            if !nonces.insert(nonce.to_string()) {
                return Err(format!(
                    "output {}: nonce {nonce} is named more than once in the file",
                    output.output_index
                ));
            }
        }
        if !sender_offsets.insert(output.sender_offset_key_id.to_string()) {
            return Err(format!(
                "output {}: sender offset key {} is named more than once in the file",
                output.output_index, output.sender_offset_key_id
            ));
        }
    }
    Ok(())
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
        ledger_output_at(3)
    }

    /// A ledger output at `index`, with keys and nonces of its own.
    fn ledger_output_at(index: u64) -> Step2OutputsForSelf {
        Step2OutputsForSelf {
            output_index: usize::try_from(index).unwrap(),
            script_nonce_key_id: ledger(LedgerKeyBranch::Random, (1 << 32) | (index << 8) | 1),
            sender_offset_key_id: ledger(
                LedgerKeyBranch::PreMine,
                PRE_MINE_SENDER_OFFSET_INDEX_BIT | (index << 8),
            ),
            sender_offset_nonce_key_id: ledger(LedgerKeyBranch::Random, (1 << 32) | (index << 8) | 2),
            pre_mine_script_key_id: ledger(LedgerKeyBranch::PreMine, index),
            ..Default::default()
        }
    }

    fn software_output_at(key_manager: &KeyManager, index: u64) -> Step2OutputsForSelf {
        let mut output = ledger_output_at(index);
        output.script_nonce_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        output.sender_offset_nonce_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        output.sender_offset_key_id = key_manager.get_random_key(None, None).unwrap().key_id;
        output
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

        let mut wrong_output_index = ledger_output();
        wrong_output_index.pre_mine_script_key_id = ledger(LedgerKeyBranch::PreMine, 4);

        let mut one_sided_sender_offset = ledger_output();
        one_sided_sender_offset.sender_offset_key_id = ledger(LedgerKeyBranch::OneSidedSenderOffset, 9);

        for (what, output) in [
            ("one nonce twice", same_nonce),
            ("a nonce off the Random branch", nonce_off_random),
            ("a marked script key", marked_script_key),
            ("an unmarked sender offset key", unmarked_sender_offset),
            ("a script key at another output's index", wrong_output_index),
            ("a OneSidedSenderOffset sender offset key", one_sided_sender_offset),
        ] {
            assert!(output.check_key_ids().is_err(), "{what} was accepted");
        }
    }

    #[test]
    fn a_valid_two_output_file_passes() {
        assert_eq!(
            check_self_file_key_ids(&[ledger_output_at(3), ledger_output_at(7)]),
            Ok(())
        );
        let key_manager = KeyManager::new_random().unwrap();
        assert_eq!(
            check_self_file_key_ids(&[software_output_at(&key_manager, 3), software_output_at(&key_manager, 7)]),
            Ok(())
        );
    }

    /// Output A on `(n1, n2)` and output B on `(n2, n1)` pass every per-output check; the file check refuses them.
    #[test]
    fn outputs_that_swap_their_nonces_are_refused() {
        let a = ledger_output_at(3);
        let mut b = ledger_output_at(7);
        b.script_nonce_key_id = a.sender_offset_nonce_key_id.clone();
        b.sender_offset_nonce_key_id = a.script_nonce_key_id.clone();
        assert_eq!(a.check_key_ids(), Ok(()));
        assert_eq!(b.check_key_ids(), Ok(()));
        assert!(check_self_file_key_ids(&[a, b]).is_err());
    }

    #[test]
    fn outputs_that_share_a_nonce_or_a_sender_offset_key_are_refused() {
        let a = ledger_output_at(3);
        let mut b = ledger_output_at(7);
        b.script_nonce_key_id = a.script_nonce_key_id.clone();
        assert!(check_self_file_key_ids(&[a.clone(), b]).is_err());

        let mut b = ledger_output_at(7);
        b.sender_offset_key_id = a.sender_offset_key_id.clone();
        assert!(check_self_file_key_ids(&[a, b]).is_err());
    }

    #[test]
    fn a_script_key_at_the_wrong_output_index_is_refused_in_a_file() {
        let a = ledger_output_at(3);
        let mut b = ledger_output_at(7);
        b.pre_mine_script_key_id = ledger(LedgerKeyBranch::PreMine, 3);
        assert!(check_self_file_key_ids(&[a, b]).is_err());
    }
}
