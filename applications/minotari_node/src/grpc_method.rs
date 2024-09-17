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

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A list of all the GRPC methods that can be enabled/disabled
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GrpcMethod {
    ListHeaders,
    GetHeaderByHash,
    GetBlocks,
    GetBlockTiming,
    GetConstants,
    GetBlockSize,
    GetBlockFees,
    #[default]
    GetVersion,
    CheckForUpdates,
    GetTokensInCirculation,
    GetNetworkDifficulty,
    GetNewBlockTemplate,
    GetNewBlock,
    GetNewBlockWithCoinbases,
    GetNewBlockTemplateWithCoinbases,
    GetNewBlockBlob,
    SubmitBlock,
    SubmitBlockBlob,
    SubmitTransaction,
    GetSyncInfo,
    GetSyncProgress,
    GetTipInfo,
    SearchKernels,
    SearchUtxos,
    FetchMatchingUtxos,
    GetPeers,
    GetMempoolTransactions,
    TransactionState,
    Identify,
    GetNetworkStatus,
    ListConnectedPeers,
    GetMempoolStats,
    GetActiveValidatorNodes,
    GetShardKey,
    GetTemplateRegistrations,
    GetSideChainUtxos,
}

impl GrpcMethod {
    /// All the GRPC methods as a fixed array
    pub const ALL_VARIANTS: [GrpcMethod; 36] = [
        GrpcMethod::ListHeaders,
        GrpcMethod::GetHeaderByHash,
        GrpcMethod::GetBlocks,
        GrpcMethod::GetBlockTiming,
        GrpcMethod::GetConstants,
        GrpcMethod::GetBlockSize,
        GrpcMethod::GetBlockFees,
        GrpcMethod::GetVersion,
        GrpcMethod::CheckForUpdates,
        GrpcMethod::GetTokensInCirculation,
        GrpcMethod::GetNetworkDifficulty,
        GrpcMethod::GetNewBlockTemplate,
        GrpcMethod::GetNewBlock,
        GrpcMethod::GetNewBlockWithCoinbases,
        GrpcMethod::GetNewBlockTemplateWithCoinbases,
        GrpcMethod::GetNewBlockBlob,
        GrpcMethod::SubmitBlock,
        GrpcMethod::SubmitBlockBlob,
        GrpcMethod::SubmitTransaction,
        GrpcMethod::GetSyncInfo,
        GrpcMethod::GetSyncProgress,
        GrpcMethod::GetTipInfo,
        GrpcMethod::SearchKernels,
        GrpcMethod::SearchUtxos,
        GrpcMethod::FetchMatchingUtxos,
        GrpcMethod::GetPeers,
        GrpcMethod::GetMempoolTransactions,
        GrpcMethod::TransactionState,
        GrpcMethod::Identify,
        GrpcMethod::GetNetworkStatus,
        GrpcMethod::ListConnectedPeers,
        GrpcMethod::GetMempoolStats,
        GrpcMethod::GetActiveValidatorNodes,
        GrpcMethod::GetShardKey,
        GrpcMethod::GetTemplateRegistrations,
        GrpcMethod::GetSideChainUtxos,
    ];
}

impl IntoIterator for GrpcMethod {
    type IntoIter = std::array::IntoIter<GrpcMethod, 36>;
    type Item = GrpcMethod;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(Self::ALL_VARIANTS)
    }
}

impl FromStr for GrpcMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Cater for 'serde_json' converted strings as well
        let binding = s.to_string().replace("\"", "");
        match binding.as_str() {
            "list_headers" => Ok(GrpcMethod::ListHeaders),
            "get_header_by_hash" => Ok(GrpcMethod::GetHeaderByHash),
            "get_blocks" => Ok(GrpcMethod::GetBlocks),
            "get_block_timing" => Ok(GrpcMethod::GetBlockTiming),
            "get_constants" => Ok(GrpcMethod::GetConstants),
            "get_block_size" => Ok(GrpcMethod::GetBlockSize),
            "get_block_fees" => Ok(GrpcMethod::GetBlockFees),
            "get_version" => Ok(GrpcMethod::GetVersion),
            "check_for_updates" => Ok(GrpcMethod::CheckForUpdates),
            "get_tokens_in_circulation" => Ok(GrpcMethod::GetTokensInCirculation),
            "get_network_difficulty" => Ok(GrpcMethod::GetNetworkDifficulty),
            "get_new_block_template" => Ok(GrpcMethod::GetNewBlockTemplate),
            "get_new_block" => Ok(GrpcMethod::GetNewBlock),
            "get_new_block_with_coinbases" => Ok(GrpcMethod::GetNewBlockWithCoinbases),
            "get_new_block_template_with_coinbases" => Ok(GrpcMethod::GetNewBlockTemplateWithCoinbases),
            "get_new_block_blob" => Ok(GrpcMethod::GetNewBlockBlob),
            "submit_block" => Ok(GrpcMethod::SubmitBlock),
            "submit_block_blob" => Ok(GrpcMethod::SubmitBlockBlob),
            "submit_transaction" => Ok(GrpcMethod::SubmitTransaction),
            "get_sync_info" => Ok(GrpcMethod::GetSyncInfo),
            "get_sync_progress" => Ok(GrpcMethod::GetSyncProgress),
            "get_tip_info" => Ok(GrpcMethod::GetTipInfo),
            "search_kernels" => Ok(GrpcMethod::SearchKernels),
            "search_utxos" => Ok(GrpcMethod::SearchUtxos),
            "fetch_matching_utxos" => Ok(GrpcMethod::FetchMatchingUtxos),
            "get_peers" => Ok(GrpcMethod::GetPeers),
            "get_mempool_transactions" => Ok(GrpcMethod::GetMempoolTransactions),
            "transaction_state" => Ok(GrpcMethod::TransactionState),
            "identify" => Ok(GrpcMethod::Identify),
            "get_network_status" => Ok(GrpcMethod::GetNetworkStatus),
            "list_connected_peers" => Ok(GrpcMethod::ListConnectedPeers),
            "get_mempool_stats" => Ok(GrpcMethod::GetMempoolStats),
            "get_active_validator_nodes" => Ok(GrpcMethod::GetActiveValidatorNodes),
            "get_shard_key" => Ok(GrpcMethod::GetShardKey),
            "get_template_registrations" => Ok(GrpcMethod::GetTemplateRegistrations),
            "get_side_chain_utxos" => Ok(GrpcMethod::GetSideChainUtxos),
            _ => Err(format!("'{}' not supported", s)),
        }
    }
}

impl fmt::Display for GrpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};

    use crate::grpc_method::GrpcMethod;

    #[derive(Clone, Serialize, Deserialize, Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct TestConfig {
        name: String,
        inner_config: TestInnerConfig,
    }

    #[derive(Clone, Serialize, Deserialize, Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct TestInnerConfig {
        allow_methods: Vec<GrpcMethod>,
    }

    #[test]
    fn it_deserializes_enums() {
        let config_str = r#"
            name = "blockchain champion"
            inner_config.allow_methods = [
                "list_headers",
                "get_constants",
            #    "get_blocks"
                "identify",
            #    "get_shard_key"
            ]
        "#;
        let config = toml::from_str::<TestConfig>(config_str).unwrap();

        // Enums in the config
        assert!(config.inner_config.allow_methods.contains(&GrpcMethod::ListHeaders));
        assert!(config.inner_config.allow_methods.contains(&GrpcMethod::GetConstants));
        assert!(!config.inner_config.allow_methods.contains(&GrpcMethod::GetBlocks)); // commented out in the config
        assert!(config.inner_config.allow_methods.contains(&GrpcMethod::Identify));
        assert!(!config.inner_config.allow_methods.contains(&GrpcMethod::GetShardKey)); // commented out in the config
    }

    #[test]
    fn grpc_method_into_iter_is_exhaustive() {
        let mut count = 0;
        for method in &GrpcMethod::ALL_VARIANTS {
            match method {
                GrpcMethod::ListHeaders => count += 1,
                GrpcMethod::GetHeaderByHash => count += 1,
                GrpcMethod::GetBlocks => count += 1,
                GrpcMethod::GetBlockTiming => count += 1,
                GrpcMethod::GetConstants => count += 1,
                GrpcMethod::GetBlockSize => count += 1,
                GrpcMethod::GetBlockFees => count += 1,
                GrpcMethod::GetVersion => count += 1,
                GrpcMethod::CheckForUpdates => count += 1,
                GrpcMethod::GetTokensInCirculation => count += 1,
                GrpcMethod::GetNetworkDifficulty => count += 1,
                GrpcMethod::GetNewBlockTemplate => count += 1,
                GrpcMethod::GetNewBlock => count += 1,
                GrpcMethod::GetNewBlockWithCoinbases => count += 1,
                GrpcMethod::GetNewBlockTemplateWithCoinbases => count += 1,
                GrpcMethod::GetNewBlockBlob => count += 1,
                GrpcMethod::SubmitBlock => count += 1,
                GrpcMethod::SubmitBlockBlob => count += 1,
                GrpcMethod::SubmitTransaction => count += 1,
                GrpcMethod::GetSyncInfo => count += 1,
                GrpcMethod::GetSyncProgress => count += 1,
                GrpcMethod::GetTipInfo => count += 1,
                GrpcMethod::SearchKernels => count += 1,
                GrpcMethod::SearchUtxos => count += 1,
                GrpcMethod::FetchMatchingUtxos => count += 1,
                GrpcMethod::GetPeers => count += 1,
                GrpcMethod::GetMempoolTransactions => count += 1,
                GrpcMethod::TransactionState => count += 1,
                GrpcMethod::Identify => count += 1,
                GrpcMethod::GetNetworkStatus => count += 1,
                GrpcMethod::ListConnectedPeers => count += 1,
                GrpcMethod::GetMempoolStats => count += 1,
                GrpcMethod::GetActiveValidatorNodes => count += 1,
                GrpcMethod::GetShardKey => count += 1,
                GrpcMethod::GetTemplateRegistrations => count += 1,
                GrpcMethod::GetSideChainUtxos => count += 1,
            }
        }
        assert_eq!(count, GrpcMethod::ALL_VARIANTS.len());
    }

    #[test]
    fn it_converts_from_serde_json_str_to_enum() {
        // Iterate over all the enum variants and convert them to a string
        for method in &GrpcMethod::ALL_VARIANTS {
            let method_str = serde_json::to_string(&method).unwrap();
            let method_from_str = GrpcMethod::from_str(&method_str).unwrap();
            assert_eq!(method, &method_from_str);
        }
    }
}
