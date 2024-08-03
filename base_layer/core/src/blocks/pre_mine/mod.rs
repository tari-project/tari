// Copyright 2024. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use rand::{prelude::SliceRandom, rngs::OsRng, thread_rng};
use tari_common_types::{
    key_branches::TransactionKeyManagerBranch,
    types::{Commitment, PrivateKey, PublicKey, Signature},
};
use tari_crypto::keys::{PublicKey as PkTrait, SecretKey as SkTrait};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{script, ExecutionStack};
use tari_utilities::ByteArray;

use crate::{
    one_sided::public_key_to_output_encryption_key,
    transactions::{
        key_manager::{
            create_memory_db_key_manager,
            SecretTransactionKeyManagerInterface,
            TransactionKeyManagerInterface,
        },
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::PaymentId,
            KernelFeatures,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputType,
            RangeProofType,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutputBuilder,
        },
        transaction_protocol::TransactionMetadata,
    },
};

/// Token unlock schedule
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnlockSchedule {
    /// Network rewards
    pub network_rewards: Apportionment,
    /// Protocol tokens
    pub protocol: Apportionment,
    /// Community tokens
    pub community: Apportionment,
    /// Contributors' tokens
    pub contributors: Apportionment,
    /// Participants' tokens
    pub participants: Apportionment,
}

/// Token apportionment
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Apportionment {
    /// Beneficiary of the apportionment
    pub beneficiary: String,
    /// Percentage of total tokens
    pub percentage: u64,
    /// Total tokens for this apportionment
    pub tokens_amount: u64,
    /// Token release cadence schedule
    pub schedule: Option<ReleaseCadence>,
}

/// Token release cadence
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReleaseCadence {
    /// Initial lockup days
    pub initial_lockup_days: u64,
    /// Monthly fraction release factor
    pub monthly_fraction_denominator: u64,
    /// Upfront release percentage
    pub upfront_release: Option<UpfrontRelease>,
}

/// The upfront percentage of the total tokens to be released
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpfrontRelease {
    /// The fraction of the total tokens to be released upfront
    pub percentage: u64,
    /// The number of tokens it has to be divided into
    pub number_of_tokens: u64,
}

/// Get the tokenomics unlock schedule as per the specification - see `https://tari.substack.com/p/tari-tokenomics`
pub fn get_tokenomics_pre_mine_unlock_schedule() -> UnlockSchedule {
    UnlockSchedule {
        network_rewards: Apportionment {
            beneficiary: "network_rewards".to_string(),
            percentage: 70,
            tokens_amount: 14_700_000_000,
            schedule: None,
        },
        protocol: Apportionment {
            beneficiary: "protocol".to_string(),
            percentage: 9,
            tokens_amount: 1_890_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 48,
                upfront_release: Some(UpfrontRelease {
                    percentage: 40,
                    number_of_tokens: 20,
                }),
            }),
        },
        community: Apportionment {
            beneficiary: "community".to_string(),
            percentage: 5,
            tokens_amount: 1_050_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 12,
                upfront_release: None,
            }),
        },
        contributors: Apportionment {
            beneficiary: "contributors".to_string(),
            percentage: 4,
            tokens_amount: 840_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 60,
                upfront_release: None,
            }),
        },
        participants: Apportionment {
            beneficiary: "participants".to_string(),
            percentage: 12,
            tokens_amount: 2_520_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 24,
                upfront_release: None,
            }),
        },
    }
}

/// Pre-mine values
#[derive(Debug)]
pub struct PreMineItem {
    pub value: MicroMinotari,
    pub maturity: u64,
    pub beneficiary: String,
}

/// Create a list of (token value, maturity in blocks) according to the amounts in the unlock schedule, based on the
/// apportionment and release cadence where 1 day equals 24 * 60 / 2 blocks.
pub fn create_pre_mine_output_values(schedule: UnlockSchedule) -> Result<Vec<PreMineItem>, String> {
    let mut values_with_maturity = Vec::new();
    let blocks_per_day = 24 * 60 / 2;
    let days_per_month = 365.25 / 12f64;
    #[allow(clippy::cast_possible_truncation)]
    let blocks_per_month = (days_per_month * blocks_per_day as f64) as u64;
    for apportionment in &[
        &schedule.network_rewards,
        &schedule.protocol,
        &schedule.community,
        &schedule.contributors,
        &schedule.participants,
    ] {
        if let Some(schedule) = apportionment.schedule.as_ref() {
            let upfront_release = schedule.upfront_release.clone().unwrap_or_default();
            if upfront_release.percentage > 100 {
                return Err(format!(
                    "Upfront percentage must be less than or equal to 100 in {:?}",
                    apportionment
                ));
            }
            if apportionment
                .tokens_amount
                .checked_mul(1_000_000 * upfront_release.percentage)
                .is_none()
            {
                return Err(format!("Minotari calculation overflow in {:?}", apportionment));
            }
            let mut tokens_value = apportionment.tokens_amount * 1_000_000;
            if upfront_release.percentage > 0 {
                let upfront_tokens = tokens_value * upfront_release.percentage / 100;
                tokens_value -= upfront_tokens;
                let value_per_round = upfront_tokens / upfront_release.number_of_tokens;
                let mut assigned_tokens = 0;
                for _ in 0..upfront_release.number_of_tokens - 1 {
                    values_with_maturity.push(PreMineItem {
                        value: MicroMinotari::from(value_per_round),
                        maturity: 0,
                        beneficiary: apportionment.beneficiary.clone(),
                    });
                    assigned_tokens += value_per_round;
                }
                values_with_maturity.push(PreMineItem {
                    value: MicroMinotari::from(upfront_tokens - assigned_tokens),
                    maturity: 0,
                    beneficiary: apportionment.beneficiary.clone(),
                });
            }
            let monthly_tokens = tokens_value / schedule.monthly_fraction_denominator;
            let mut total_tokens = 0;
            let mut maturity = 0;
            for i in 0..schedule.monthly_fraction_denominator - 1 {
                total_tokens += monthly_tokens;
                maturity = schedule.initial_lockup_days * blocks_per_day + i * blocks_per_month;
                values_with_maturity.push(PreMineItem {
                    value: MicroMinotari::from(monthly_tokens),
                    maturity,
                    beneficiary: apportionment.beneficiary.clone(),
                });
            }
            let last_tokens = tokens_value - total_tokens;
            values_with_maturity.push(PreMineItem {
                value: MicroMinotari::from(last_tokens),
                maturity: maturity + blocks_per_month,
                beneficiary: apportionment.beneficiary.clone(),
            });
        }
    }
    Ok(values_with_maturity)
}

/// Get the pre-mine items according to the pre-mine specification
pub async fn get_pre_mine_items() -> Result<Vec<PreMineItem>, String> {
    let schedule = get_tokenomics_pre_mine_unlock_schedule();
    create_pre_mine_output_values(schedule)
}

// The threshold is 1 more than half of the public keys if even, otherwise 1 more than half of 'public keys - 1'
fn get_signature_threshold(number_of_keys: usize) -> Result<u8, String> {
    if number_of_keys < 2 {
        return Err("Invalid number of parties, must be > 1".to_string());
    }
    u8::try_from(number_of_keys / 2 + 1).map_err(|e| e.to_string())
}

/// Create a pre-mine genesis block file with the given pre-mine items and party public keys
pub async fn create_pre_mine_genesis_block_file(
    pre_mine_items: &[PreMineItem],
    threshold_spend_keys: &[Vec<PublicKey>],
    backup_spend_keys: &[PublicKey],
) -> Result<(Vec<TransactionOutput>, TransactionKernel), String> {
    // 1 month fail-safe height for each pre-mine output at which time the backup_address can spend the output
    let fail_safe_height = 720 * 30;
    let mut outputs = Vec::new();
    let mut total_private_key = PrivateKey::default();
    for (i, ((item, public_keys), backup_key)) in pre_mine_items
        .iter()
        .zip(threshold_spend_keys)
        .zip(backup_spend_keys)
        .enumerate()
    {
        let signature_threshold = get_signature_threshold(public_keys.len())?;
        let total_script_key = public_keys.iter().fold(PublicKey::default(), |acc, x| acc + x);
        let key_manager = create_memory_db_key_manager().unwrap();
        let view_key = public_key_to_output_encryption_key(&total_script_key).unwrap();
        let view_key_id = key_manager.import_key(view_key.clone()).await.unwrap();
        let address_len = u8::try_from(public_keys.len()).unwrap();

        let (commitment_mask, script_key) = key_manager.get_next_commitment_mask_and_script_key().await.unwrap();
        total_private_key = total_private_key + &key_manager.get_private_key(&commitment_mask.key_id).await.unwrap();
        let commitment = key_manager
            .get_commitment(&commitment_mask.key_id, &item.value.into())
            .await
            .unwrap();
        let mut commitment_bytes = [0u8; 32];
        commitment_bytes.clone_from_slice(commitment.as_bytes());

        let sender_offset = key_manager
            .get_next_key(TransactionKeyManagerBranch::SenderOffset.get_branch_key())
            .await
            .unwrap();
        let mut public_keys = public_keys.clone();
        public_keys.shuffle(&mut thread_rng());
        let script = script!(
            CheckHeight(item.maturity + fail_safe_height) LeZero
            IfThen
            CheckMultiSigVerifyAggregatePubKey(signature_threshold, address_len, public_keys.clone(), Box::new(commitment_bytes))
            Else
            PushPubKey(Box::new(backup_key.clone()))
            EndIf
        );
        let output = WalletOutputBuilder::new(item.value, commitment_mask.key_id)
            .with_features(OutputFeatures::new(
                OutputFeaturesVersion::get_current_version(),
                OutputType::Standard,
                item.maturity,
                Vec::new(),
                None,
                RangeProofType::RevealedValue,
            ))
            .with_script(script)
            .encrypt_data_for_recovery(&key_manager, Some(&view_key_id), PaymentId::U64(i.try_into().unwrap()))
            .await
            .unwrap()
            .with_input_data(ExecutionStack::default())
            .with_version(TransactionOutputVersion::get_current_version())
            .with_sender_offset_public_key(sender_offset.pub_key)
            .with_script_key(script_key.key_id)
            .with_minimum_value_promise(item.value)
            .sign_as_sender_and_receiver(&key_manager, &sender_offset.key_id)
            .await
            .unwrap()
            .try_build(&key_manager)
            .await
            .unwrap();
        outputs.push(output.to_transaction_output(&key_manager).await.unwrap());
    }
    // lets create a single kernel for all the outputs
    let r = PrivateKey::random(&mut OsRng);
    let tx_meta = TransactionMetadata::new_with_features(0.into(), 0, KernelFeatures::empty());
    let total_public_key = PublicKey::from_secret_key(&total_private_key);
    let e = TransactionKernel::build_kernel_challenge_from_tx_meta(
        &TransactionKernelVersion::get_current_version(),
        &PublicKey::from_secret_key(&r),
        &total_public_key,
        &tx_meta,
    );
    let signature = Signature::sign_raw_uniform(&total_private_key, r, &e).unwrap();
    let excess = Commitment::from_public_key(&total_public_key);
    let kernel = TransactionKernel::new_current_version(KernelFeatures::empty(), 0.into(), 0, excess, signature, None);
    Ok((outputs, kernel))
}

#[cfg(test)]
mod test {
    use std::{fs, fs::File, io::Write};

    use tari_common_types::tari_address::TariAddress;

    use crate::{
        blocks::pre_mine::{
            create_pre_mine_genesis_block_file,
            create_pre_mine_output_values,
            get_signature_threshold,
            get_tokenomics_pre_mine_unlock_schedule,
            Apportionment,
            PreMineItem,
            ReleaseCadence,
            UpfrontRelease,
        },
        transactions::tari_amount::MicroMinotari,
    };

    // Only run this when you want to create a new utxo file
    #[ignore]
    #[tokio::test]
    async fn print_pre_mine() {
        let addresses_for_round = vec![
            // This wil be public keys
            TariAddress::from_base58(
                "f4bYsv3sEMroDGKMMjhgm7cp1jDShdRWQzmV8wZiD6sJPpAEuezkiHtVhn7akK3YqswH5t3sUASW7rbvPSqMBDSCSp",
            )
            .unwrap(),
            TariAddress::from_base58(
                "f44jftbpTid23oDsEjTodayvMmudSr3g66R6scTJkB5911ZfJRq32FUJDD4CiQSkAPq574i8pMjqzm5RtzdH3Kuknwz",
            )
            .unwrap(),
            TariAddress::from_base58(
                "f4GYN3QVRboH6uwG9oFj3LjmUd4XVd1VDYiT6rNd4gCpZF6pY7iuoCpoajfDfuPynS7kspXU5hKRMWLTP9CRjoe1hZU",
            )
            .unwrap(),
        ];
        let backup_address = TariAddress::from_base58(
            "f4GYN3QVRboH6uwG9oFj3LjmUd4XVd1VDYiT6rNd4gCpZF6pY7iuoCpoajfDfuPynS7kspXU5hKRMWLTP9CRjoe1hZU",
        )
        .unwrap();
        let public_spend_keys_for_round: Vec<_> = addresses_for_round
            .iter()
            .map(|address| address.public_spend_key().clone())
            .collect();

        let schedule = get_tokenomics_pre_mine_unlock_schedule();
        let mut pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
        // Add some test outputs
        for _ in 0..20 {
            pre_mine_items.push(PreMineItem {
                value: MicroMinotari::from(1_000_000),
                maturity: 0,
                beneficiary: "test_output".to_string(),
            });
        }
        let mut public_spend_keys = Vec::with_capacity(pre_mine_items.len());
        let mut backup_public_spend_keys = Vec::with_capacity(pre_mine_items.len());
        for _ in 0..pre_mine_items.len() {
            public_spend_keys.push(public_spend_keys_for_round.clone());
            backup_public_spend_keys.push(backup_address.public_spend_key().clone());
        }

        let (outputs, kernel) =
            create_pre_mine_genesis_block_file(&pre_mine_items, &public_spend_keys, &backup_public_spend_keys)
                .await
                .unwrap();

        let base_dir = dirs_next::document_dir().unwrap();
        let file_path = base_dir.join("tari_pre_mine").join("create").join("utxos.json");
        if let Some(path) = file_path.parent() {
            if !path.exists() {
                fs::create_dir_all(path).unwrap();
            }
        }
        let mut utxo_file = File::create(&file_path).expect("Could not create 'utxos.json'");

        for output in outputs {
            let utxo_s = serde_json::to_string(&output).unwrap();
            utxo_file.write_all(format!("{}\n", utxo_s).as_bytes()).unwrap();
        }

        let kernel = serde_json::to_string(&kernel).unwrap();
        let _result = utxo_file.write_all(format!("{}\n", kernel).as_bytes());
        println!(
            "\nOutputs written to: '{}'\n",
            fs::canonicalize(&file_path).unwrap().display()
        );
    }

    #[test]
    fn test_get_tokenomics_pre_mine_unlock_schedule() {
        let schedule = get_tokenomics_pre_mine_unlock_schedule();
        assert_eq!(schedule.network_rewards, Apportionment {
            beneficiary: "network_rewards".to_string(),
            percentage: 70,
            tokens_amount: 14_700_000_000,
            schedule: None,
        });
        assert_eq!(schedule.protocol, Apportionment {
            beneficiary: "protocol".to_string(),
            percentage: 9,
            tokens_amount: 1_890_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 48,
                upfront_release: Some(UpfrontRelease {
                    percentage: 40,
                    number_of_tokens: 20
                }),
            }),
        });
        assert_eq!(
            schedule.protocol.tokens_amount * schedule.protocol.schedule.unwrap().upfront_release.unwrap().percentage /
                100,
            756_000_000
        );
        assert_eq!(schedule.community, Apportionment {
            beneficiary: "community".to_string(),
            percentage: 5,
            tokens_amount: 1_050_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 12,
                upfront_release: None,
            }),
        });
        assert_eq!(schedule.contributors, Apportionment {
            beneficiary: "contributors".to_string(),
            percentage: 4,
            tokens_amount: 840_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 60,
                upfront_release: None,
            }),
        });
        assert_eq!(schedule.participants, Apportionment {
            beneficiary: "participants".to_string(),
            percentage: 12,
            tokens_amount: 2_520_000_000,
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 24,
                upfront_release: None,
            }),
        });

        assert_eq!(
            schedule.participants.percentage +
                schedule.contributors.percentage +
                schedule.community.percentage +
                schedule.protocol.percentage +
                schedule.network_rewards.percentage,
            100
        );

        assert_eq!(
            schedule.participants.tokens_amount +
                schedule.contributors.tokens_amount +
                schedule.community.tokens_amount +
                schedule.protocol.tokens_amount +
                schedule.network_rewards.tokens_amount,
            21_000_000_000
        );
    }

    #[test]
    fn test_create_pre_mine_output_values() {
        let schedule = get_tokenomics_pre_mine_unlock_schedule();
        let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
        for item in &pre_mine_items {
            println!("{:?}", item);
        }

        // Verify pre_mine items as per `https://tari.substack.com/p/tari-tokenomics`
        let total_pre_mine_value = pre_mine_items.iter().map(|item| item.value).sum::<MicroMinotari>();
        let total_tokens = schedule.network_rewards.tokens_amount +
            schedule.protocol.tokens_amount +
            schedule.community.tokens_amount +
            schedule.contributors.tokens_amount +
            schedule.participants.tokens_amount;
        let total_value = MicroMinotari::from(total_tokens * 1_000_000);
        assert_eq!(
            total_pre_mine_value + MicroMinotari::from(schedule.network_rewards.tokens_amount * 1_000_000),
            total_value
        );
        let protocol_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "protocol")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            protocol_tokens,
            MicroMinotari::from(schedule.protocol.tokens_amount * 1_000_000)
        );
        let protocol_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "protocol" && item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(protocol_tokens_at_start, MicroMinotari::from(756_000_000 * 1_000_000));
        let all_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(all_tokens_at_start, MicroMinotari::from(756_000_000 * 1_000_000));
        let community_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "community")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            community_tokens,
            MicroMinotari::from(schedule.community.tokens_amount * 1_000_000)
        );
        let contributors_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "contributors")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            contributors_tokens,
            MicroMinotari::from(schedule.contributors.tokens_amount * 1_000_000)
        );
        let participants_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "participants")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            participants_tokens,
            MicroMinotari::from(schedule.participants.tokens_amount * 1_000_000)
        );
    }

    #[test]
    fn test_get_signature_threshold() {
        assert!(get_signature_threshold(0).is_err());
        assert!(get_signature_threshold(1).is_err());
        assert_eq!(get_signature_threshold(2).unwrap(), 2);
        assert_eq!(get_signature_threshold(3).unwrap(), 2);
        assert_eq!(get_signature_threshold(4).unwrap(), 3);
        assert_eq!(get_signature_threshold(5).unwrap(), 3);
        assert_eq!(get_signature_threshold(6).unwrap(), 4);
        assert_eq!(get_signature_threshold(7).unwrap(), 4);
        assert_eq!(get_signature_threshold(8).unwrap(), 5);
        assert_eq!(get_signature_threshold(9).unwrap(), 5);
        assert_eq!(get_signature_threshold(10).unwrap(), 6);
        assert_eq!(get_signature_threshold(11).unwrap(), 6);
        assert_eq!(get_signature_threshold(12).unwrap(), 7);
        assert_eq!(get_signature_threshold(13).unwrap(), 7);
        assert_eq!(get_signature_threshold(14).unwrap(), 8);
        assert_eq!(get_signature_threshold(15).unwrap(), 8);
        assert_eq!(get_signature_threshold(16).unwrap(), 9);
        assert_eq!(get_signature_threshold(17).unwrap(), 9);
        assert_eq!(get_signature_threshold(18).unwrap(), 10);
        assert_eq!(get_signature_threshold(19).unwrap(), 10);
        assert_eq!(get_signature_threshold(20).unwrap(), 11);
    }
}
