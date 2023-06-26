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

use std::{collections::HashMap, convert::TryFrom, iter::FromIterator};

use tari_core::{consensus::ConsensusConstants, proof_of_work::PowAlgorithm};

use crate::tari_rpc as grpc;

impl From<ConsensusConstants> for grpc::ConsensusConstants {
    #[allow(clippy::too_many_lines)]
    fn from(cc: ConsensusConstants) -> Self {
        let (emission_initial, emission_decay, emission_tail) = cc.emission_amounts();
        let weight_params = cc.transaction_weight_params().params();
        let input_version_range = cc.input_version_range().clone().into_inner();
        let input_version_range = grpc::Range {
            min: u64::from(input_version_range.0.as_u8()),
            max: u64::from(input_version_range.1.as_u8()),
        };
        let kernel_version_range = cc.kernel_version_range().clone().into_inner();
        let kernel_version_range = grpc::Range {
            min: u64::from(kernel_version_range.0.as_u8()),
            max: u64::from(kernel_version_range.1.as_u8()),
        };
        let valid_blockchain_version_range = cc.valid_blockchain_version_range().clone().into_inner();
        let valid_blockchain_version_range = grpc::Range {
            min: u64::from(valid_blockchain_version_range.0),
            max: u64::from(valid_blockchain_version_range.1),
        };
        let transaction_weight = cc.transaction_weight_params();
        let features_and_scripts_bytes_per_gram = transaction_weight.params().features_and_scripts_bytes_per_gram.get();
        let transaction_weight = grpc::WeightParams {
            kernel_weight: cc.transaction_weight_params().params().kernel_weight,
            input_weight: cc.transaction_weight_params().params().input_weight,
            output_weight: cc.transaction_weight_params().params().output_weight,
            features_and_scripts_bytes_per_gram,
        };
        let output_version_range = cc.output_version_range();
        let outputs = grpc::Range {
            min: u64::from(output_version_range.outputs.start().as_u8()),
            max: u64::from(output_version_range.outputs.end().as_u8()),
        };
        let features = grpc::Range {
            min: u64::from(output_version_range.features.start().as_u8()),
            max: u64::from(output_version_range.features.end().as_u8()),
        };

        let output_version_range = grpc::OutputsVersion {
            outputs: Some(outputs),
            features: Some(features),
        };

        let permitted_output_types = cc.permitted_output_types();
        let permitted_output_types = permitted_output_types
            .iter()
            .map(|ot| i32::from(ot.as_byte()))
            .collect::<Vec<i32>>();

        let permitted_range_proof_types = cc.permitted_range_proof_types();
        let permitted_range_proof_types = permitted_range_proof_types
            .iter()
            .map(|rpt| i32::from(rpt.as_byte()))
            .collect::<Vec<i32>>();

        let monero_pow = PowAlgorithm::Monero;
        let sha3_pow = PowAlgorithm::Sha3;

        let monero_pow = grpc::PowAlgorithmConstants {
            max_difficulty: cc.max_pow_difficulty(monero_pow).as_u64(),
            min_difficulty: cc.min_pow_difficulty(monero_pow).as_u64(),
            target_time: cc.pow_target_block_interval(monero_pow),
        };

        let sha3_pow = grpc::PowAlgorithmConstants {
            max_difficulty: cc.max_pow_difficulty(sha3_pow).as_u64(),
            min_difficulty: cc.min_pow_difficulty(sha3_pow).as_u64(),
            target_time: cc.pow_target_block_interval(sha3_pow),
        };

        let proof_of_work = HashMap::from_iter([(0u32, monero_pow), (1u32, sha3_pow)]);

        Self {
            coinbase_min_maturity: cc.coinbase_min_maturity(),
            blockchain_version: cc.blockchain_version().into(),
            future_time_limit: cc.ftl().as_u64(),
            difficulty_block_window: cc.get_difficulty_block_window(),
            max_block_transaction_weight: cc.get_max_block_transaction_weight(),
            pow_algo_count: cc.get_pow_algo_count(),
            median_timestamp_count: u64::try_from(cc.get_median_timestamp_count()).unwrap_or(0),
            emission_initial: emission_initial.into(),
            emission_decay: emission_decay.to_vec(),
            emission_tail: emission_tail.into(),
            min_blake_pow_difficulty: cc.min_pow_difficulty(PowAlgorithm::Sha3).into(),
            block_weight_inputs: weight_params.input_weight,
            block_weight_outputs: weight_params.output_weight,
            block_weight_kernels: weight_params.kernel_weight,
            max_script_byte_size: cc.get_max_script_byte_size() as u64,
            faucet_value: cc.faucet_value().as_u64(),
            effective_from_height: cc.effective_from_height(),
            input_version_range: Some(input_version_range),
            kernel_version_range: Some(kernel_version_range),
            valid_blockchain_version_range: Some(valid_blockchain_version_range),
            proof_of_work,
            transaction_weight: Some(transaction_weight),
            max_randomx_seed_height: cc.max_randomx_seed_height(),
            output_version_range: Some(output_version_range),
            permitted_output_types,
            permitted_range_proof_types,
            validator_node_validity_period: cc.validator_node_validity_period_epochs().as_u64(),
            epoch_length: cc.epoch_length(),
            validator_node_registration_min_deposit_amount: cc
                .validator_node_registration_min_deposit_amount()
                .as_u64(),
            validator_node_registration_min_lock_height: cc.validator_node_registration_min_lock_height(),
            validator_node_registration_shuffle_interval_epoch: cc
                .validator_node_registration_shuffle_interval()
                .as_u64(),
        }
    }
}
