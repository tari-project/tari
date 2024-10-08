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

use std::{
    convert::{TryFrom, TryInto},
    iter::once,
};

use rand::{prelude::SliceRandom, rngs::OsRng, thread_rng};
use tari_common::configuration::Network;
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
        tari_amount::{MicroMinotari, Minotari},
        transaction_components::{
            encrypted_data::PaymentId,
            CoinBaseExtra,
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

const BLOCKS_PER_DAY: u64 = 24 * 60 / 2;

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
    pub tokens_amount: Minotari,
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
    pub upfront_release: Vec<ReleaseStrategy>,
    /// Expected payout period in blocks from after the initial lockup
    pub expected_payout_period_blocks: u64,
}

/// The upfront release of tokens
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReleaseStrategy {
    /// Proportional upfront release
    Proportional(ProportionalRelease),
    /// Custom specified upfront release
    Custom(Vec<CustomRelease>),
    /// Upfront release taken from the regular cadence
    FromCadence(Vec<CadenceRelease>),
}

impl Default for ReleaseStrategy {
    fn default() -> Self {
        Self::Proportional(ProportionalRelease::default())
    }
}

/// The upfront tokens to be released on a proportional basis
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProportionalRelease {
    /// The fraction of the total tokens to be released upfront
    pub percentage: u64,
    /// The number of tokens it has to be divided into
    pub number_of_tokens: u64,
}

/// The upfront tokens to be released on a custom schedule
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomRelease {
    /// The value of the token
    pub value: Minotari,
    /// The maturity of the token
    pub maturity: u64,
}

/// The upfront tokens to be released on a cadence basis, where the token value is removed from a specific period in the
/// parent schedule
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CadenceRelease {
    /// The value of the token
    pub value: Minotari,
    /// The period in the release cadence the token is taken from
    pub taken_from_period: u64,
    /// The maturity of the token
    pub maturity: u64,
}

fn get_expected_payout_grace_period_blocks(network: Network) -> u64 {
    match network {
        Network::MainNet => {
            BLOCKS_PER_DAY * 30 * 6 // 6 months
        },
        _ => {
            BLOCKS_PER_DAY // 1 day
        },
    }
}

/// Get the tokenomics unlock schedule as per the specification - see `https://tari.substack.com/p/tari-tokenomics`
pub fn get_tokenomics_pre_mine_unlock_schedule(network: Network) -> UnlockSchedule {
    UnlockSchedule {
        network_rewards: Apportionment {
            beneficiary: "network_rewards".to_string(),
            percentage: 70,
            tokens_amount: 14_700_000_000.into(),
            schedule: None,
        },
        protocol: Apportionment {
            beneficiary: "protocol".to_string(),
            percentage: 9,
            tokens_amount: 1_890_000_000.into(),
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 48,
                upfront_release: vec![
                    ReleaseStrategy::Proportional(ProportionalRelease {
                        percentage: 40,
                        number_of_tokens: 20,
                    }),
                    ReleaseStrategy::Custom({
                        // 129,600 = 720 (blocks per day) * 30 (days per month) * 6 (months)
                        vec![
                            CustomRelease {
                                value: 1.into(),
                                maturity: 0,
                            },
                            CustomRelease {
                                value: 1.into(),
                                maturity: 0,
                            },
                            CustomRelease {
                                value: 1.into(),
                                maturity: 129_600,
                            },
                            CustomRelease {
                                value: 1.into(),
                                maturity: 129_600,
                            },
                        ]
                    }),
                ],
                expected_payout_period_blocks: get_expected_payout_grace_period_blocks(network),
            }),
        },
        community: Apportionment {
            beneficiary: "community".to_string(),
            percentage: 5,
            tokens_amount: 1_050_000_000.into(),
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 180,
                monthly_fraction_denominator: 12,
                upfront_release: vec![],
                expected_payout_period_blocks: get_expected_payout_grace_period_blocks(network),
            }),
        },
        contributors: Apportionment {
            beneficiary: "contributors".to_string(),
            percentage: 4,
            tokens_amount: 840_000_000.into(),
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 60,
                upfront_release: contributors_upfront_release(),
                expected_payout_period_blocks: get_expected_payout_grace_period_blocks(network),
            }),
        },
        participants: Apportionment {
            beneficiary: "participants".to_string(),
            percentage: 12,
            tokens_amount: 2_520_000_000.into(),
            schedule: Some(ReleaseCadence {
                initial_lockup_days: 365,
                monthly_fraction_denominator: 24,
                upfront_release: vec![],
                expected_payout_period_blocks: get_expected_payout_grace_period_blocks(network),
            }),
        },
    }
}

#[rustfmt::skip]
#[allow(clippy::too_many_lines)]
fn contributors_upfront_release() -> Vec<ReleaseStrategy> {
    vec![
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 809_645.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 809_645.into(), taken_from_period: 12, maturity: 0 },
            ]
        }),
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 824_673.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 1_005_513.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 741_964.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 765_682.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 726_947.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 532_348.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 901_887.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 847_583.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 574_566.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 579_726.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 960_845.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 873_277.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 644_305.into(), taken_from_period: 12, maturity: 0 },
                CadenceRelease { value: 851_701.into(), taken_from_period: 13, maturity: 0 },
                CadenceRelease { value: 604_768.into(), taken_from_period: 14, maturity: 0 },
                CadenceRelease { value: 736_635.into(), taken_from_period: 15, maturity: 0 },
                CadenceRelease { value: 579_131.into(), taken_from_period: 16, maturity: 0 },
                CadenceRelease { value: 988_598.into(), taken_from_period: 17, maturity: 0 },
                CadenceRelease { value: 889_487.into(), taken_from_period: 18, maturity: 0 },
                CadenceRelease { value: 753_657.into(), taken_from_period: 19, maturity: 0 },
                CadenceRelease { value: 759_468.into(), taken_from_period: 20, maturity: 0 },
                CadenceRelease { value: 941_477.into(), taken_from_period: 21, maturity: 0 },
                CadenceRelease { value: 727_783.into(), taken_from_period: 22, maturity: 0 },
                CadenceRelease { value: 890_507.into(), taken_from_period: 23, maturity: 0 },
                CadenceRelease { value: 614_537.into(), taken_from_period: 24, maturity: 0 },
                CadenceRelease { value: 982_928.into(), taken_from_period: 25, maturity: 0 },
                CadenceRelease { value: 683_333.into(), taken_from_period: 26, maturity: 0 },
                CadenceRelease { value: 608_822.into(), taken_from_period: 27, maturity: 0 },
                CadenceRelease { value: 833_976.into(), taken_from_period: 28, maturity: 0 },
                CadenceRelease { value: 921_427.into(), taken_from_period: 29, maturity: 0 },
                CadenceRelease { value: 533_382.into(), taken_from_period: 30, maturity: 0 },
                CadenceRelease { value: 777_393.into(), taken_from_period: 31, maturity: 0 },
                CadenceRelease { value: 991_867.into(), taken_from_period: 32, maturity: 0 },
                CadenceRelease { value: 656_632.into(), taken_from_period: 33, maturity: 0 },
                CadenceRelease { value: 964_390.into(), taken_from_period: 34, maturity: 0 },
                CadenceRelease { value: 685_446.into(), taken_from_period: 35, maturity: 0 },
                CadenceRelease { value: 754_340.into(), taken_from_period: 36, maturity: 0 },
                CadenceRelease { value: 696_425.into(), taken_from_period: 37, maturity: 0 },
                CadenceRelease { value: 961_729.into(), taken_from_period: 38, maturity: 0 },
                CadenceRelease { value: 979_717.into(), taken_from_period: 39, maturity: 0 },
                CadenceRelease { value: 707_969.into(), taken_from_period: 40, maturity: 0 },
                CadenceRelease { value: 578_193.into(), taken_from_period: 41, maturity: 0 },
                CadenceRelease { value: 746_015.into(), taken_from_period: 42, maturity: 0 },
                CadenceRelease { value: 660_210.into(), taken_from_period: 43, maturity: 0 },
                CadenceRelease { value: 969_205.into(), taken_from_period: 44, maturity: 0 },
                CadenceRelease { value: 546_862.into(), taken_from_period: 45, maturity: 0 },
                CadenceRelease { value: 944_554.into(), taken_from_period: 46, maturity: 0 },
                CadenceRelease { value: 783_055.into(), taken_from_period: 47, maturity: 0 },
                CadenceRelease { value: 565_248.into(), taken_from_period: 48, maturity: 0 },
                CadenceRelease { value: 565_419.into(), taken_from_period: 49, maturity: 0 },
                CadenceRelease { value: 841_914.into(), taken_from_period: 50, maturity: 0 },
                CadenceRelease { value: 533_128.into(), taken_from_period: 51, maturity: 0 },
                CadenceRelease { value: 763_864.into(), taken_from_period: 52, maturity: 0 },
                CadenceRelease { value: 670_235.into(), taken_from_period: 53, maturity: 0 },
                CadenceRelease { value: 553_652.into(), taken_from_period: 54, maturity: 0 },
                CadenceRelease { value: 994_209.into(), taken_from_period: 55, maturity: 0 },
                CadenceRelease { value: 573_788.into(), taken_from_period: 56, maturity: 0 },
                CadenceRelease { value: 558_331.into(), taken_from_period: 57, maturity: 0 },
                CadenceRelease { value: 661_775.into(), taken_from_period: 58, maturity: 0 },
                CadenceRelease { value: 660_229.into(), taken_from_period: 59, maturity: 0 },
            ]
        }),
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 813_643.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 881_823.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 789_591.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 1_022_883.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 1_030_418.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 926_181.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 577_256.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 788_166.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 838_888.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 763_625.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 972_443.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 917_705.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 525_627.into(), taken_from_period: 12, maturity: 0 },
                CadenceRelease { value: 725_903.into(), taken_from_period: 13, maturity: 0 },
                CadenceRelease { value: 648_237.into(), taken_from_period: 14, maturity: 0 },
                CadenceRelease { value: 709_630.into(), taken_from_period: 15, maturity: 0 },
                CadenceRelease { value: 640_568.into(), taken_from_period: 16, maturity: 0 },
                CadenceRelease { value: 681_965.into(), taken_from_period: 17, maturity: 0 },
                CadenceRelease { value: 602_616.into(), taken_from_period: 18, maturity: 0 },
                CadenceRelease { value: 803_544.into(), taken_from_period: 19, maturity: 0 },
                CadenceRelease { value: 535_435.into(), taken_from_period: 20, maturity: 0 },
                CadenceRelease { value: 960_539.into(), taken_from_period: 21, maturity: 0 },
                CadenceRelease { value: 805_552.into(), taken_from_period: 22, maturity: 0 },
                CadenceRelease { value: 564_642.into(), taken_from_period: 23, maturity: 0 },
                CadenceRelease { value: 851_760.into(), taken_from_period: 24, maturity: 0 },
                CadenceRelease { value: 550_421.into(), taken_from_period: 25, maturity: 0 },
                CadenceRelease { value: 616_102.into(), taken_from_period: 26, maturity: 0 },
                CadenceRelease { value: 798_368.into(), taken_from_period: 27, maturity: 0 },
                CadenceRelease { value: 661_595.into(), taken_from_period: 28, maturity: 0 },
                CadenceRelease { value: 592_393.into(), taken_from_period: 29, maturity: 0 },
                CadenceRelease { value: 892_637.into(), taken_from_period: 30, maturity: 0 },
                CadenceRelease { value: 1_044_084.into(), taken_from_period: 31, maturity: 0 },
                CadenceRelease { value: 921_493.into(), taken_from_period: 32, maturity: 0 },
                CadenceRelease { value: 695_985.into(), taken_from_period: 33, maturity: 0 },
                CadenceRelease { value: 664_685.into(), taken_from_period: 34, maturity: 0 },
                CadenceRelease { value: 944_098.into(), taken_from_period: 35, maturity: 0 },
                CadenceRelease { value: 739_786.into(), taken_from_period: 36, maturity: 0 },
                CadenceRelease { value: 1_022_001.into(), taken_from_period: 37, maturity: 0 },
                CadenceRelease { value: 674_768.into(), taken_from_period: 38, maturity: 0 },
                CadenceRelease { value: 733_062.into(), taken_from_period: 39, maturity: 0 },
                CadenceRelease { value: 903_737.into(), taken_from_period: 40, maturity: 0 },
                CadenceRelease { value: 917_629.into(), taken_from_period: 41, maturity: 0 },
                CadenceRelease { value: 732_956.into(), taken_from_period: 42, maturity: 0 },
                CadenceRelease { value: 817_190.into(), taken_from_period: 43, maturity: 0 },
                CadenceRelease { value: 966_635.into(), taken_from_period: 44, maturity: 0 },
                CadenceRelease { value: 750_136.into(), taken_from_period: 45, maturity: 0 },
                CadenceRelease { value: 580_861.into(), taken_from_period: 46, maturity: 0 },
                CadenceRelease { value: 562_952.into(), taken_from_period: 47, maturity: 0 },
                CadenceRelease { value: 630_510.into(), taken_from_period: 48, maturity: 0 },
                CadenceRelease { value: 782_673.into(), taken_from_period: 49, maturity: 0 },
                CadenceRelease { value: 835_188.into(), taken_from_period: 50, maturity: 0 },
                CadenceRelease { value: 715_922.into(), taken_from_period: 51, maturity: 0 },
                CadenceRelease { value: 777_730.into(), taken_from_period: 52, maturity: 0 },
                CadenceRelease { value: 921_451.into(), taken_from_period: 53, maturity: 0 },
                CadenceRelease { value: 924_002.into(), taken_from_period: 54, maturity: 0 },
                CadenceRelease { value: 797_222.into(), taken_from_period: 55, maturity: 0 },
                CadenceRelease { value: 1_004_380.into(), taken_from_period: 56, maturity: 0 },
                CadenceRelease { value: 662_620.into(), taken_from_period: 57, maturity: 0 },
                CadenceRelease { value: 541_377.into(), taken_from_period: 58, maturity: 0 },
                CadenceRelease { value: 677_586.into(), taken_from_period: 59, maturity: 0 },
            ]
        }),
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 1_014_986.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 579_490.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 628_144.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 782_380.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 726_853.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 1_041_368.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 738_040.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 535_090.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 927_994.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 1_019_140.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 819_700.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 801_647.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 810_409.into(), taken_from_period: 12, maturity: 0 },
                CadenceRelease { value: 909_958.into(), taken_from_period: 13, maturity: 0 },
                CadenceRelease { value: 710_557.into(), taken_from_period: 14, maturity: 0 },
                CadenceRelease { value: 678_670.into(), taken_from_period: 15, maturity: 0 },
                CadenceRelease { value: 585_242.into(), taken_from_period: 16, maturity: 0 },
                CadenceRelease { value: 953_777.into(), taken_from_period: 17, maturity: 0 },
                CadenceRelease { value: 923_917.into(), taken_from_period: 18, maturity: 0 },
                CadenceRelease { value: 775_199.into(), taken_from_period: 19, maturity: 0 },
                CadenceRelease { value: 1_003_184.into(), taken_from_period: 20, maturity: 0 },
                CadenceRelease { value: 918_275.into(), taken_from_period: 21, maturity: 0 },
                CadenceRelease { value: 565_154.into(), taken_from_period: 22, maturity: 0 },
                CadenceRelease { value: 694_683.into(), taken_from_period: 23, maturity: 0 },
                CadenceRelease { value: 1_027_322.into(), taken_from_period: 24, maturity: 0 },
                CadenceRelease { value: 570_680.into(), taken_from_period: 25, maturity: 0 },
                CadenceRelease { value: 762_277.into(), taken_from_period: 26, maturity: 0 },
                CadenceRelease { value: 657_084.into(), taken_from_period: 27, maturity: 0 },
                CadenceRelease { value: 736_547.into(), taken_from_period: 28, maturity: 0 },
                CadenceRelease { value: 810_347.into(), taken_from_period: 29, maturity: 0 },
                CadenceRelease { value: 764_813.into(), taken_from_period: 30, maturity: 0 },
                CadenceRelease { value: 534_728.into(), taken_from_period: 31, maturity: 0 },
                CadenceRelease { value: 925_511.into(), taken_from_period: 32, maturity: 0 },
                CadenceRelease { value: 654_460.into(), taken_from_period: 33, maturity: 0 },
                CadenceRelease { value: 753_327.into(), taken_from_period: 34, maturity: 0 },
                CadenceRelease { value: 685_149.into(), taken_from_period: 35, maturity: 0 },
                CadenceRelease { value: 537_099.into(), taken_from_period: 36, maturity: 0 },
                CadenceRelease { value: 751_173.into(), taken_from_period: 37, maturity: 0 },
                CadenceRelease { value: 659_700.into(), taken_from_period: 38, maturity: 0 },
                CadenceRelease { value: 599_538.into(), taken_from_period: 39, maturity: 0 },
                CadenceRelease { value: 715_361.into(), taken_from_period: 40, maturity: 0 },
                CadenceRelease { value: 939_556.into(), taken_from_period: 41, maturity: 0 },
                CadenceRelease { value: 850_652.into(), taken_from_period: 42, maturity: 0 },
                CadenceRelease { value: 821_581.into(), taken_from_period: 43, maturity: 0 },
                CadenceRelease { value: 1_025_818.into(), taken_from_period: 44, maturity: 0 },
                CadenceRelease { value: 534_215.into(), taken_from_period: 45, maturity: 0 },
                CadenceRelease { value: 648_920.into(), taken_from_period: 46, maturity: 0 },
                CadenceRelease { value: 928_382.into(), taken_from_period: 47, maturity: 0 },
                CadenceRelease { value: 631_160.into(), taken_from_period: 48, maturity: 0 },
                CadenceRelease { value: 934_093.into(), taken_from_period: 49, maturity: 0 },
                CadenceRelease { value: 557_917.into(), taken_from_period: 50, maturity: 0 },
                CadenceRelease { value: 952_918.into(), taken_from_period: 51, maturity: 0 },
                CadenceRelease { value: 958_366.into(), taken_from_period: 52, maturity: 0 },
                CadenceRelease { value: 764_993.into(), taken_from_period: 53, maturity: 0 },
                CadenceRelease { value: 666_649.into(), taken_from_period: 54, maturity: 0 },
                CadenceRelease { value: 865_950.into(), taken_from_period: 55, maturity: 0 },
                CadenceRelease { value: 723_407.into(), taken_from_period: 56, maturity: 0 },
                CadenceRelease { value: 813_615.into(), taken_from_period: 57, maturity: 0 },
                CadenceRelease { value: 917_188.into(), taken_from_period: 58, maturity: 0 },
                CadenceRelease { value: 924_610.into(), taken_from_period: 59, maturity: 0 },
            ]
        }),
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 616_830.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 797_418.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 941_993.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 709_938.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 572_674.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 639_504.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 572_953.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 666_495.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 719_367.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 930_783.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 613_101.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 673_805.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 744_488.into(), taken_from_period: 12, maturity: 0 },
                CadenceRelease { value: 1_013_088.into(), taken_from_period: 13, maturity: 0 },
                CadenceRelease { value: 720_550.into(), taken_from_period: 14, maturity: 0 },
                CadenceRelease { value: 796_775.into(), taken_from_period: 15, maturity: 0 },
                CadenceRelease { value: 795_532.into(), taken_from_period: 16, maturity: 0 },
                CadenceRelease { value: 756_581.into(), taken_from_period: 17, maturity: 0 },
                CadenceRelease { value: 932_389.into(), taken_from_period: 18, maturity: 0 },
                CadenceRelease { value: 1_030_437.into(), taken_from_period: 19, maturity: 0 },
                CadenceRelease { value: 811_102.into(), taken_from_period: 20, maturity: 0 },
                CadenceRelease { value: 532_754.into(), taken_from_period: 21, maturity: 0 },
                CadenceRelease { value: 803_929.into(), taken_from_period: 22, maturity: 0 },
                CadenceRelease { value: 548_344.into(), taken_from_period: 23, maturity: 0 },
                CadenceRelease { value: 581_285.into(), taken_from_period: 24, maturity: 0 },
                CadenceRelease { value: 900_542.into(), taken_from_period: 25, maturity: 0 },
                CadenceRelease { value: 1_030_901.into(), taken_from_period: 26, maturity: 0 },
                CadenceRelease { value: 608_663.into(), taken_from_period: 27, maturity: 0 },
                CadenceRelease { value: 972_302.into(), taken_from_period: 28, maturity: 0 },
                CadenceRelease { value: 777_592.into(), taken_from_period: 29, maturity: 0 },
                CadenceRelease { value: 558_963.into(), taken_from_period: 30, maturity: 0 },
                CadenceRelease { value: 1_017_575.into(), taken_from_period: 31, maturity: 0 },
                CadenceRelease { value: 768_931.into(), taken_from_period: 32, maturity: 0 },
                CadenceRelease { value: 836_563.into(), taken_from_period: 33, maturity: 0 },
                CadenceRelease { value: 591_157.into(), taken_from_period: 34, maturity: 0 },
                CadenceRelease { value: 920_889.into(), taken_from_period: 35, maturity: 0 },
                CadenceRelease { value: 788_603.into(), taken_from_period: 36, maturity: 0 },
                CadenceRelease { value: 797_334.into(), taken_from_period: 37, maturity: 0 },
                CadenceRelease { value: 1_029_718.into(), taken_from_period: 38, maturity: 0 },
                CadenceRelease { value: 866_889.into(), taken_from_period: 39, maturity: 0 },
                CadenceRelease { value: 628_497.into(), taken_from_period: 40, maturity: 0 },
                CadenceRelease { value: 805_441.into(), taken_from_period: 41, maturity: 0 },
                CadenceRelease { value: 809_036.into(), taken_from_period: 42, maturity: 0 },
                CadenceRelease { value: 756_271.into(), taken_from_period: 43, maturity: 0 },
                CadenceRelease { value: 746_495.into(), taken_from_period: 44, maturity: 0 },
                CadenceRelease { value: 1_019_658.into(), taken_from_period: 45, maturity: 0 },
                CadenceRelease { value: 991_395.into(), taken_from_period: 46, maturity: 0 },
                CadenceRelease { value: 618_737.into(), taken_from_period: 47, maturity: 0 },
                CadenceRelease { value: 648_297.into(), taken_from_period: 48, maturity: 0 },
                CadenceRelease { value: 777_555.into(), taken_from_period: 49, maturity: 0 },
                CadenceRelease { value: 931_514.into(), taken_from_period: 50, maturity: 0 },
                CadenceRelease { value: 855_201.into(), taken_from_period: 51, maturity: 0 },
                CadenceRelease { value: 956_589.into(), taken_from_period: 52, maturity: 0 },
                CadenceRelease { value: 917_116.into(), taken_from_period: 53, maturity: 0 },
                CadenceRelease { value: 569_886.into(), taken_from_period: 54, maturity: 0 },
                CadenceRelease { value: 834_817.into(), taken_from_period: 55, maturity: 0 },
                CadenceRelease { value: 623_698.into(), taken_from_period: 56, maturity: 0 },
                CadenceRelease { value: 554_924.into(), taken_from_period: 57, maturity: 0 },
                CadenceRelease { value: 913_323.into(), taken_from_period: 58, maturity: 0 },
                CadenceRelease { value: 612_031.into(), taken_from_period: 59, maturity: 0 },
            ]
        }),
        ReleaseStrategy::FromCadence({
            vec![
                CadenceRelease { value: 929_868.into(), taken_from_period: 0, maturity: 0 },
                CadenceRelease { value: 935_756.into(), taken_from_period: 1, maturity: 0 },
                CadenceRelease { value: 1_098_308.into(), taken_from_period: 2, maturity: 0 },
                CadenceRelease { value: 919_117.into(), taken_from_period: 3, maturity: 0 },
                CadenceRelease { value: 1_143_108.into(), taken_from_period: 4, maturity: 0 },
                CadenceRelease { value: 1_060_599.into(), taken_from_period: 5, maturity: 0 },
                CadenceRelease { value: 1_409_864.into(), taken_from_period: 6, maturity: 0 },
                CadenceRelease { value: 1_362_666.into(), taken_from_period: 7, maturity: 0 },
                CadenceRelease { value: 1_139_185.into(), taken_from_period: 8, maturity: 0 },
                CadenceRelease { value: 906_726.into(), taken_from_period: 9, maturity: 0 },
                CadenceRelease { value: 833_911.into(), taken_from_period: 10, maturity: 0 },
                CadenceRelease { value: 933_566.into(), taken_from_period: 11, maturity: 0 },
                CadenceRelease { value: 1_475_171.into(), taken_from_period: 12, maturity: 0 },
                CadenceRelease { value: 699_350.into(), taken_from_period: 13, maturity: 0 },
                CadenceRelease { value: 1_515_888.into(), taken_from_period: 14, maturity: 0 },
                CadenceRelease { value: 1_278_290.into(), taken_from_period: 15, maturity: 0 },
                CadenceRelease { value: 1_599_527.into(), taken_from_period: 16, maturity: 0 },
                CadenceRelease { value: 819_079.into(), taken_from_period: 17, maturity: 0 },
                CadenceRelease { value: 851_591.into(), taken_from_period: 18, maturity: 0 },
                CadenceRelease { value: 837_163.into(), taken_from_period: 19, maturity: 0 },
                CadenceRelease { value: 1_090_811.into(), taken_from_period: 20, maturity: 0 },
                CadenceRelease { value: 846_955.into(), taken_from_period: 21, maturity: 0 },
                CadenceRelease { value: 1_297_582.into(), taken_from_period: 22, maturity: 0 },
                CadenceRelease { value: 1_501_824.into(), taken_from_period: 23, maturity: 0 },
                CadenceRelease { value: 1_125_096.into(), taken_from_period: 24, maturity: 0 },
                CadenceRelease { value: 1_195_429.into(), taken_from_period: 25, maturity: 0 },
                CadenceRelease { value: 1_107_387.into(), taken_from_period: 26, maturity: 0 },
                CadenceRelease { value: 1_527_063.into(), taken_from_period: 27, maturity: 0 },
                CadenceRelease { value: 995_580.into(), taken_from_period: 28, maturity: 0 },
                CadenceRelease { value: 1_098_241.into(), taken_from_period: 29, maturity: 0 },
                CadenceRelease { value: 1_450_205.into(), taken_from_period: 30, maturity: 0 },
                CadenceRelease { value: 826_220.into(), taken_from_period: 31, maturity: 0 },
                CadenceRelease { value: 592_198.into(), taken_from_period: 32, maturity: 0 },
                CadenceRelease { value: 1_356_360.into(), taken_from_period: 33, maturity: 0 },
                CadenceRelease { value: 1_226_441.into(), taken_from_period: 34, maturity: 0 },
                CadenceRelease { value: 964_418.into(), taken_from_period: 35, maturity: 0 },
                CadenceRelease { value: 1_380_172.into(), taken_from_period: 36, maturity: 0 },
                CadenceRelease { value: 933_067.into(), taken_from_period: 37, maturity: 0 },
                CadenceRelease { value: 874_085.into(), taken_from_period: 38, maturity: 0 },
                CadenceRelease { value: 1_020_794.into(), taken_from_period: 39, maturity: 0 },
                CadenceRelease { value: 1_244_436.into(), taken_from_period: 40, maturity: 0 },
                CadenceRelease { value: 959_181.into(), taken_from_period: 41, maturity: 0 },
                CadenceRelease { value: 1_061_341.into(), taken_from_period: 42, maturity: 0 },
                CadenceRelease { value: 1_144_748.into(), taken_from_period: 43, maturity: 0 },
                CadenceRelease { value: 491_847.into(), taken_from_period: 44, maturity: 0 },
                CadenceRelease { value: 1_349_129.into(), taken_from_period: 45, maturity: 0 },
                CadenceRelease { value: 1_034_270.into(), taken_from_period: 46, maturity: 0 },
                CadenceRelease { value: 1_306_874.into(), taken_from_period: 47, maturity: 0 },
                CadenceRelease { value: 1_724_785.into(), taken_from_period: 48, maturity: 0 },
                CadenceRelease { value: 1_140_260.into(), taken_from_period: 49, maturity: 0 },
                CadenceRelease { value: 1_033_467.into(), taken_from_period: 50, maturity: 0 },
                CadenceRelease { value: 1_142_831.into(), taken_from_period: 51, maturity: 0 },
                CadenceRelease { value: 743_451.into(), taken_from_period: 52, maturity: 0 },
                CadenceRelease { value: 926_205.into(), taken_from_period: 53, maturity: 0 },
                CadenceRelease { value: 1_485_811.into(), taken_from_period: 54, maturity: 0 },
                CadenceRelease { value: 707_802.into(), taken_from_period: 55, maturity: 0 },
                CadenceRelease { value: 1_274_727.into(), taken_from_period: 56, maturity: 0 },
                CadenceRelease { value: 1_610_510.into(), taken_from_period: 57, maturity: 0 },
                CadenceRelease { value: 1_166_337.into(), taken_from_period: 58, maturity: 0 },
                CadenceRelease { value: 1_325_544.into(), taken_from_period: 59, maturity: 0 },
            ]
        }),
    ]
}

/// Pre-mine values
#[derive(Debug)]
pub struct PreMineItem {
    /// The value of the pre-mine
    pub value: MicroMinotari,
    /// The maturity of the pre-mine at which it can be spend
    pub maturity: u64,
    /// The original maturity of the pre-mine taken into account any pre-release strategy
    pub original_maturity: u64,
    /// The fail-safe height (absolute height) at which the pre-mine can be spent by a backup or fail-safe wallet
    pub fail_safe_height: u64,
    /// The beneficiary of the pre-mine
    pub beneficiary: String,
}

/// Get the total pre-mine value
pub fn get_pre_mine_value(network: Network) -> Result<MicroMinotari, String> {
    let schedule = get_tokenomics_pre_mine_unlock_schedule(network);
    let pre_mine_items = create_pre_mine_output_values(schedule.clone())?;
    Ok(pre_mine_items.iter().map(|item| item.value).sum::<MicroMinotari>())
}

struct CadenceItem {
    taken_from_period: u64,
    value: Minotari,
}

/// Create a list of (token value, maturity in blocks) according to the amounts in the unlock schedule, based on the
/// apportionment and release cadence where 1 day equals 24 * 60 / 2 blocks.
#[allow(clippy::too_many_lines)]
pub fn create_pre_mine_output_values(schedule: UnlockSchedule) -> Result<Vec<PreMineItem>, String> {
    let mut values_with_maturity = Vec::new();
    let days_per_month = 365.25 / 12f64;
    #[allow(clippy::cast_possible_truncation)]
    let blocks_per_month = (days_per_month * BLOCKS_PER_DAY as f64) as u64;
    for apportionment in &[
        &schedule.network_rewards,
        &schedule.protocol,
        &schedule.community,
        &schedule.contributors,
        &schedule.participants,
    ] {
        if let Some(schedule) = apportionment.schedule.as_ref() {
            let mut tokens_value = apportionment.tokens_amount.uT().as_u64();
            let mut early_payout = Vec::new();

            // Upfront release
            for item in &schedule.upfront_release {
                match item {
                    ReleaseStrategy::Proportional(upfront_release) => {
                        if upfront_release.percentage > 100 {
                            return Err(format!(
                                "Upfront percentage must be less than or equal to 100 in {:?}",
                                apportionment
                            ));
                        }
                        if apportionment
                            .tokens_amount
                            .uT()
                            .as_u64()
                            .checked_mul(upfront_release.percentage)
                            .is_none()
                        {
                            return Err(format!("Minotari calculation overflow in {:?}", apportionment));
                        }
                        if upfront_release.percentage > 0 {
                            let upfront_tokens = tokens_value * upfront_release.percentage / 100;
                            tokens_value -= upfront_tokens;
                            let value_per_round = upfront_tokens / upfront_release.number_of_tokens;
                            let mut assigned_tokens = 0;
                            for _ in 0..upfront_release.number_of_tokens - 1 {
                                values_with_maturity.push(PreMineItem {
                                    value: MicroMinotari::from(value_per_round),
                                    maturity: 0,
                                    original_maturity: 0,
                                    fail_safe_height: schedule.expected_payout_period_blocks,
                                    beneficiary: apportionment.beneficiary.clone(),
                                });
                                assigned_tokens += value_per_round;
                            }
                            values_with_maturity.push(PreMineItem {
                                value: MicroMinotari::from(upfront_tokens - assigned_tokens),
                                maturity: 0,
                                original_maturity: 0,
                                fail_safe_height: schedule.expected_payout_period_blocks,
                                beneficiary: apportionment.beneficiary.clone(),
                            });
                        }
                    },
                    ReleaseStrategy::Custom(upfront_release) => {
                        for release in upfront_release {
                            tokens_value -= release.value.uT().as_u64();
                            values_with_maturity.push(PreMineItem {
                                value: release.value.uT(),
                                maturity: release.maturity,
                                original_maturity: release.maturity,
                                fail_safe_height: release.maturity + schedule.expected_payout_period_blocks,
                                beneficiary: apportionment.beneficiary.clone(),
                            });
                        }
                    },
                    ReleaseStrategy::FromCadence(upfront_release) => {
                        for release in upfront_release {
                            early_payout.push(CadenceItem {
                                taken_from_period: release.taken_from_period,
                                value: release.value,
                            });
                            let original_maturity = schedule.initial_lockup_days * BLOCKS_PER_DAY +
                                release.taken_from_period * blocks_per_month;
                            values_with_maturity.push(PreMineItem {
                                value: release.value.uT(),
                                maturity: release.maturity,
                                original_maturity,
                                fail_safe_height: original_maturity + schedule.expected_payout_period_blocks,
                                beneficiary: apportionment.beneficiary.clone(),
                            });
                        }
                    },
                }
            }

            // Combine all upfront 'ReleaseStrategy::FromCadence' payouts into a single value per period
            early_payout.sort_by_key(|x| x.taken_from_period);
            let mut periods = early_payout
                .iter()
                .map(|item| item.taken_from_period)
                .collect::<Vec<_>>();
            periods.dedup();
            let mut early_payouts_summed = Vec::with_capacity(periods.len());
            for period in periods {
                let period_value: Minotari = MicroMinotari::from(
                    early_payout
                        .iter()
                        .filter(|item| item.taken_from_period == period)
                        .map(|item| item.value.uT().as_u64())
                        .sum::<u64>(),
                )
                .into();
                early_payouts_summed.push(CadenceItem {
                    taken_from_period: period,
                    value: period_value,
                });
            }

            // Monthly release
            let monthly_tokens = tokens_value / schedule.monthly_fraction_denominator;
            let mut total_tokens = 0;
            let mut maturity = 0;
            for i in 0..schedule.monthly_fraction_denominator - 1 {
                total_tokens += monthly_tokens;
                maturity = schedule.initial_lockup_days * BLOCKS_PER_DAY + i * blocks_per_month;
                let adjusted_monthly_tokens =
                    if let Some(payout) = early_payouts_summed.iter().find(|item| item.taken_from_period == i) {
                        if payout.value.uT().as_u64() >= monthly_tokens {
                            return Err(format!(
                                "upfront 'FromCadence' payout exceeds allocated monthly payout {}, allocated: {}, \
                                 early payout {}",
                                i,
                                MicroMinotari::from(monthly_tokens),
                                payout.value.uT()
                            ));
                        }
                        monthly_tokens - payout.value.uT().as_u64()
                    } else {
                        monthly_tokens
                    };
                values_with_maturity.push(PreMineItem {
                    value: MicroMinotari::from(adjusted_monthly_tokens),
                    maturity,
                    original_maturity: maturity,
                    fail_safe_height: maturity + schedule.expected_payout_period_blocks,
                    beneficiary: apportionment.beneficiary.clone(),
                });
            }
            let last_tokens = tokens_value - total_tokens;
            let adjusted_last_tokens = if let Some(payout) = early_payouts_summed
                .iter()
                .find(|item| item.taken_from_period == schedule.monthly_fraction_denominator - 1)
            {
                if payout.value.uT().as_u64() >= last_tokens {
                    return Err(format!(
                        "upfront 'FromCadence' payout exceeds allocated monthly payout {}, allocated: {}, early \
                         payout {}",
                        schedule.monthly_fraction_denominator - 1,
                        MicroMinotari::from(last_tokens),
                        payout.value.uT()
                    ));
                }
                last_tokens - payout.value.uT().as_u64()
            } else {
                last_tokens
            };
            maturity += blocks_per_month;
            values_with_maturity.push(PreMineItem {
                value: MicroMinotari::from(adjusted_last_tokens),
                maturity,
                original_maturity: maturity,
                fail_safe_height: maturity + schedule.expected_payout_period_blocks,
                beneficiary: apportionment.beneficiary.clone(),
            });
        }
    }
    Ok(values_with_maturity)
}

/// Get the pre-mine items according to the pre-mine specification
pub async fn get_pre_mine_items(network: Network) -> Result<Vec<PreMineItem>, String> {
    let schedule = get_tokenomics_pre_mine_unlock_schedule(network);
    create_pre_mine_output_values(schedule)
}

// The threshold is 1 more than half of the public keys if even, otherwise 1 more than half of 'public keys - 1'
fn get_signature_threshold(number_of_keys: usize) -> Result<u8, String> {
    if number_of_keys < 2 {
        return Err("Invalid number of parties, must be > 1".to_string());
    }
    u8::try_from(number_of_keys / 2 + 1).map_err(|e| e.to_string())
}

/// Verify that the script keys for the given index match the expected keys
pub fn verify_script_keys_for_index(
    index: usize,
    script_threshold_keys: &[PublicKey],
    script_backup_key: &PublicKey,
    expected_threshold_keys: &[PublicKey],
    expected_backup_key: &PublicKey,
) -> Result<(), String> {
    let mut all_script_keys = script_threshold_keys
        .iter()
        .chain(once(script_backup_key))
        .cloned()
        .collect::<Vec<_>>();
    let mut all_expected_keys = expected_threshold_keys
        .iter()
        .chain(once(expected_backup_key))
        .cloned()
        .collect::<Vec<_>>();
    all_script_keys.sort();
    all_expected_keys.sort();
    if all_script_keys.len() != all_expected_keys.len() {
        return Err(format!(
            "Output at index {} script key count mismatch ({} != {})",
            index,
            all_script_keys.len(),
            all_expected_keys.len()
        ));
    }
    all_script_keys.dedup();
    if all_expected_keys.len() != all_script_keys.len() {
        return Err(format!("Output at index {} script keys not unique", index));
    }
    for (index, (script_key, party_key)) in all_script_keys.iter().zip(all_expected_keys).enumerate() {
        if script_key != &party_key {
            return Err(format!(
                "\nError: Output {} script key mismatch ({} != {})\n",
                index, script_key, party_key
            ));
        }
    }

    Ok(())
}

/// Create pre-mine genesis block info with the given pre-mine items and party public keys
pub async fn create_pre_mine_genesis_block_info(
    pre_mine_items: &[PreMineItem],
    threshold_spend_keys: &[Vec<PublicKey>],
    backup_spend_keys: &[PublicKey],
) -> Result<(Vec<TransactionOutput>, TransactionKernel), String> {
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
            CheckHeight(item.fail_safe_height) LeZero
            IfThen
            CheckMultiSigVerifyAggregatePubKey(signature_threshold, address_len, public_keys.clone(), Box::new(commitment_bytes))
            Else
            PushPubKey(Box::new(backup_key.clone()))
            EndIf
        ).map_err(|e| e.to_string())?;
        let output = WalletOutputBuilder::new(item.value, commitment_mask.key_id)
            .with_features(OutputFeatures::new(
                OutputFeaturesVersion::get_current_version(),
                OutputType::Standard,
                item.maturity,
                CoinBaseExtra::default(),
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
    use std::{fs, fs::File, io::Write, ops::Deref};

    use tari_common::configuration::Network;
    use tari_common_types::{tari_address::TariAddress, types::PublicKey};
    use tari_script::{Opcode, Opcode::CheckHeight};

    use crate::{
        blocks::pre_mine::{
            contributors_upfront_release,
            create_pre_mine_genesis_block_info,
            create_pre_mine_output_values,
            get_expected_payout_grace_period_blocks,
            get_pre_mine_value,
            get_signature_threshold,
            get_tokenomics_pre_mine_unlock_schedule,
            verify_script_keys_for_index,
            Apportionment,
            CustomRelease,
            PreMineItem,
            ProportionalRelease,
            ReleaseCadence,
            ReleaseStrategy,
            BLOCKS_PER_DAY,
        },
        consensus::consensus_constants::MAINNET_PRE_MINE_VALUE,
        transactions::{
            tari_amount::{MicroMinotari, Minotari},
            transaction_components::{TransactionKernel, TransactionOutput},
        },
    };

    async fn genesis_block_test_info(
        pre_mine_items: &[PreMineItem],
    ) -> (
        Vec<TransactionOutput>,
        TransactionKernel,
        Vec<Vec<PublicKey>>,
        Vec<PublicKey>,
    ) {
        let threshold_addresses_for_index = vec![
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
        let backup_address_for_index = TariAddress::from_base58(
            "f27nBFv1GQBW6SuPPCUhjpLRJm3Y5uJxhbEh5EHkunsgqEi78mvzZ7uH1eEuLoRLWVSAeZTKP1BCQyrjeRJZW2pr6DR",
        )
        .unwrap();
        let threshold_spend_keys_for_index: Vec<_> = threshold_addresses_for_index
            .iter()
            .map(|address| address.public_spend_key().clone())
            .collect();

        let mut threshold_spend_keys = Vec::with_capacity(pre_mine_items.len());
        let mut backup_spend_keys = Vec::with_capacity(pre_mine_items.len());
        for _ in 0..pre_mine_items.len() {
            threshold_spend_keys.push(threshold_spend_keys_for_index.clone());
            backup_spend_keys.push(backup_address_for_index.public_spend_key().clone());
        }

        let (outputs, kernel) =
            create_pre_mine_genesis_block_info(pre_mine_items, &threshold_spend_keys, &backup_spend_keys)
                .await
                .unwrap();
        (outputs, kernel, threshold_spend_keys, backup_spend_keys)
    }

    // Only run this when you want to create a new utxo file
    #[ignore]
    #[tokio::test]
    async fn print_pre_mine_genesis_block_test_info() {
        let schedule = get_tokenomics_pre_mine_unlock_schedule(Network::MainNet);
        let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
        let (outputs, kernel, _, _) = genesis_block_test_info(&pre_mine_items).await;
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

    #[ignore]
    #[tokio::test]
    async fn print_pre_mine_list() {
        let schedule = get_tokenomics_pre_mine_unlock_schedule(Network::MainNet);
        let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
        let base_dir = dirs_next::document_dir().unwrap();
        let file_path = base_dir.join("tari_pre_mine").join("create").join("pre_mine_items.csv");
        if let Some(path) = file_path.parent() {
            if !path.exists() {
                fs::create_dir_all(path).unwrap();
            }
        }
        let mut file_stream = File::create(&file_path).expect("Could not create 'utxos.json'");

        file_stream
            .write_all("index,value,maturity,original_maturity,fail_safe_height,beneficiary\n".as_bytes())
            .unwrap();
        for (index, item) in pre_mine_items.iter().enumerate() {
            file_stream
                .write_all(
                    format!(
                        "{},{},{},{},{},{}\n",
                        index,
                        item.value,
                        item.maturity,
                        item.original_maturity,
                        item.fail_safe_height,
                        item.beneficiary,
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_get_tokenomics_pre_mine_unlock_schedule() {
        for network in [
            Network::LocalNet,
            Network::MainNet,
            Network::Esmeralda,
            Network::Igor,
            Network::NextNet,
            Network::StageNet,
        ] {
            let expected_payout_period_blocks = match network {
                Network::MainNet => {
                    BLOCKS_PER_DAY * 30 * 6 // 6 months
                },
                _ => {
                    BLOCKS_PER_DAY // 1 day
                },
            };
            let schedule = get_tokenomics_pre_mine_unlock_schedule(network);
            assert_eq!(schedule.network_rewards, Apportionment {
                beneficiary: "network_rewards".to_string(),
                percentage: 70,
                tokens_amount: 14_700_000_000.into(),
                schedule: None,
            });
            assert_eq!(schedule.protocol, Apportionment {
                beneficiary: "protocol".to_string(),
                percentage: 9,
                tokens_amount: 1_890_000_000.into(),
                schedule: Some(ReleaseCadence {
                    initial_lockup_days: 180,
                    monthly_fraction_denominator: 48,
                    upfront_release: vec![
                        ReleaseStrategy::Proportional(ProportionalRelease {
                            percentage: 40,
                            number_of_tokens: 20,
                        }),
                        ReleaseStrategy::Custom({
                            vec![
                                CustomRelease {
                                    value: 1.into(),
                                    maturity: 0,
                                },
                                CustomRelease {
                                    value: 1.into(),
                                    maturity: 0,
                                },
                                CustomRelease {
                                    value: 1.into(),
                                    maturity: 129_600,
                                },
                                CustomRelease {
                                    value: 1.into(),
                                    maturity: 129_600,
                                },
                            ]
                        }),
                    ],
                    expected_payout_period_blocks,
                }),
            });
            let percentage = if let ReleaseStrategy::Proportional(release) =
                &schedule.protocol.schedule.unwrap().upfront_release[0]
            {
                release.percentage
            } else {
                panic!("Expected ReleaseStrategy::Proportional");
            };
            assert_eq!(schedule.protocol.tokens_amount * percentage / 100, 756_000_000.into());
            assert_eq!(schedule.community, Apportionment {
                beneficiary: "community".to_string(),
                percentage: 5,
                tokens_amount: 1_050_000_000.into(),
                schedule: Some(ReleaseCadence {
                    initial_lockup_days: 180,
                    monthly_fraction_denominator: 12,
                    upfront_release: vec![],
                    expected_payout_period_blocks,
                }),
            });
            assert_eq!(schedule.contributors, Apportionment {
                beneficiary: "contributors".to_string(),
                percentage: 4,
                tokens_amount: 840_000_000.into(),
                schedule: Some(ReleaseCadence {
                    initial_lockup_days: 365,
                    monthly_fraction_denominator: 60,
                    upfront_release: contributors_upfront_release(),
                    expected_payout_period_blocks,
                }),
            });
            assert_eq!(schedule.participants, Apportionment {
                beneficiary: "participants".to_string(),
                percentage: 12,
                tokens_amount: 2_520_000_000.into(),
                schedule: Some(ReleaseCadence {
                    initial_lockup_days: 365,
                    monthly_fraction_denominator: 24,
                    upfront_release: vec![],
                    expected_payout_period_blocks,
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
                21_000_000_000.into()
            );
        }
    }

    #[test]
    fn test_create_pre_mine_total_value() {
        for network in [
            Network::LocalNet,
            Network::MainNet,
            Network::Esmeralda,
            Network::Igor,
            Network::NextNet,
            Network::StageNet,
        ] {
            let total_pre_mine_value = get_pre_mine_value(network).unwrap();
            assert_eq!(total_pre_mine_value, MAINNET_PRE_MINE_VALUE)
        }
    }

    #[test]
    fn test_contributors_upfront_release_totals() {
        let upfront_release = contributors_upfront_release();
        let min_period = upfront_release
            .iter()
            .filter_map(|strategy| match strategy {
                ReleaseStrategy::FromCadence(item) => item.iter().map(|entry| entry.taken_from_period).min(),
                _ => None,
            })
            .min()
            .unwrap();
        assert_eq!(min_period, 0);

        let max_period = upfront_release
            .iter()
            .filter_map(|strategy| match strategy {
                ReleaseStrategy::FromCadence(item) => item.iter().map(|entry| entry.taken_from_period).max(),
                _ => None,
            })
            .max()
            .unwrap();
        assert_eq!(max_period, 59);

        let mut total_value = 0;
        for (i, item) in upfront_release.iter().enumerate() {
            let tranche_value = match item {
                ReleaseStrategy::FromCadence(item) => item.iter().map(|entry| entry.value.uT().as_u64()).sum::<u64>(),
                _ => 0,
            };
            total_value += tranche_value;
            match i {
                0 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(10_525_385)).as_u64()),
                1 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(45_226_727)).as_u64()),
                2 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(46_432_875)).as_u64()),
                3 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(46_748_963)).as_u64()),
                4 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(46_559_218)).as_u64()),
                5 => assert_eq!(tranche_value, MicroMinotari::from(Minotari::from(67_032_217)).as_u64()),
                _ => panic!("Unexpected upfront release strategy"),
            }
        }
        assert_eq!(total_value, MicroMinotari::from(Minotari::from(262_525_385)).as_u64());

        for i in 0..60 {
            let value_per_period = upfront_release
                .iter()
                .map(|entry| match entry {
                    ReleaseStrategy::FromCadence(item) => item
                        .iter()
                        .filter(|entry| entry.taken_from_period == i)
                        .map(|entry| MicroMinotari::from(entry.value).as_u64())
                        .collect::<Vec<_>>()
                        .iter()
                        .sum::<u64>(),
                    _ => 0,
                })
                .collect::<Vec<_>>()
                .iter()
                .sum::<u64>();
            if i <= 12 {
                assert_eq!(
                    value_per_period,
                    MicroMinotari::from(Minotari::from(809_645 + 4_200_000)).as_u64()
                );
            } else {
                assert_eq!(
                    value_per_period,
                    MicroMinotari::from(Minotari::from(4_200_000)).as_u64()
                );
            }
        }
    }

    #[test]
    fn test_pre_mine_fail_safe_height() {
        for network in [
            Network::LocalNet,
            Network::MainNet,
            Network::Esmeralda,
            Network::Igor,
            Network::NextNet,
            Network::StageNet,
        ] {
            let schedule = get_tokenomics_pre_mine_unlock_schedule(network);
            let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
            for item in pre_mine_items {
                assert_eq!(
                    item.fail_safe_height,
                    item.original_maturity + get_expected_payout_grace_period_blocks(network)
                );
            }
        }
    }

    #[test]
    fn test_create_pre_mine_output_values() {
        let schedule = get_tokenomics_pre_mine_unlock_schedule(Network::default());
        let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();

        // Verify pre_mine items as per `https://tari.substack.com/p/tari-tokenomics`
        let total_pre_mine_value = get_pre_mine_value(Network::default()).unwrap();
        let total_tokens = schedule.network_rewards.tokens_amount +
            schedule.protocol.tokens_amount +
            schedule.community.tokens_amount +
            schedule.contributors.tokens_amount +
            schedule.participants.tokens_amount;
        let total_value = MicroMinotari::from(total_tokens);
        assert_eq!(
            total_pre_mine_value + MicroMinotari::from(schedule.network_rewards.tokens_amount),
            total_value
        );
        let protocol_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "protocol")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(protocol_tokens, MicroMinotari::from(schedule.protocol.tokens_amount));

        let protocol_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "protocol" && item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(protocol_tokens_at_start, MicroMinotari::from(756_000_002 * 1_000_000));
        let community_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "community" && item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(community_tokens_at_start, MicroMinotari::zero());
        let contributors_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "contributors" && item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            contributors_tokens_at_start,
            MicroMinotari::from(262_525_385 * 1_000_000)
        );
        let participants_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "participants" && item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(participants_tokens_at_start, MicroMinotari::zero());
        let all_tokens_at_start = pre_mine_items
            .iter()
            .filter(|item| item.maturity == 0)
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            all_tokens_at_start,
            protocol_tokens_at_start +
                community_tokens_at_start +
                contributors_tokens_at_start +
                participants_tokens_at_start
        );

        let community_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "community")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(community_tokens, MicroMinotari::from(schedule.community.tokens_amount));
        let contributors_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "contributors")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            contributors_tokens,
            MicroMinotari::from(schedule.contributors.tokens_amount)
        );
        let participants_tokens = pre_mine_items
            .iter()
            .filter(|item| item.beneficiary == "participants")
            .map(|item| item.value)
            .sum::<MicroMinotari>();
        assert_eq!(
            participants_tokens,
            MicroMinotari::from(schedule.participants.tokens_amount)
        );
    }

    #[ignore]
    #[tokio::test]
    async fn test_create_genesis_block_info() {
        for network in [
            Network::LocalNet,
            Network::MainNet,
            Network::Esmeralda,
            Network::Igor,
            Network::NextNet,
            Network::StageNet,
        ] {
            let schedule = get_tokenomics_pre_mine_unlock_schedule(network);
            let pre_mine_items = create_pre_mine_output_values(schedule.clone()).unwrap();
            let (outputs, kernel, threshold_spend_keys, backup_spend_keys) =
                genesis_block_test_info(&pre_mine_items).await;
            assert!(kernel.verify_signature().is_ok());
            let grace_period = get_expected_payout_grace_period_blocks(network);
            for (index, (output, (pre_mine_item, (threshold_keys, backup_key)))) in outputs
                .iter()
                .zip(
                    pre_mine_items
                        .iter()
                        .zip(threshold_spend_keys.iter().zip(backup_spend_keys.iter())),
                )
                .enumerate()
            {
                let script_height = if let Some(CheckHeight(height)) = output.script.as_slice().first() {
                    *height
                } else {
                    panic!("Expected CheckHeight opcode in script at index {}", index);
                };
                let script_threshold_keys =
                    if let Some(Opcode::CheckMultiSigVerifyAggregatePubKey(_n, _m, keys, _msg)) =
                        output.script.as_slice().get(3)
                    {
                        keys.clone()
                    } else {
                        panic!(
                            "Expected CheckMultiSigVerifyAggregatePubKey opcode in script at index {}",
                            index
                        );
                    };
                let script_backup_key = if let Some(Opcode::PushPubKey(key)) = output.script.as_slice().get(5) {
                    key.deref().clone()
                } else {
                    panic!("Expected PushPubKey opcode in script at index {}", index);
                };
                assert_eq!(script_height, pre_mine_item.original_maturity + grace_period);
                assert_eq!(output.features.maturity, pre_mine_item.maturity);
                assert!(verify_script_keys_for_index(
                    index,
                    &script_threshold_keys,
                    &script_backup_key,
                    threshold_keys,
                    backup_key
                )
                .is_ok());
            }
        }
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
