// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// A list of all the Console Wallet GRPC methods that can be enabled/disabled.
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WalletGrpcMethod {
    #[default]
    GetVersion,
    GetState,
    CheckConnectivity,
    CheckForUpdates,
    Identify,
    GetAddress,
    GetPaymentIdAddress,
    GetCompleteAddress,
    PrepareOneSidedTransactionForSigning,
    PrepareDepositMultisigTransaction,
    PrepareWithdrawMultisigTransaction,
    BroadcastSignedOneSidedTransaction,
    Transfer,
    RangeLimitedCoinJoin,
    GetTransactionInfo,
    GetCompletedTransactions,
    GetBlockHeightTransactions,
    GetTransactionPayRefs,
    GetBalance,
    GetUnspentAmounts,
    CoinSplit,
    CoinHistogram,
    ImportUtxos,
    GetNetworkStatus,
    GetConnectedHttpPeer,
    CancelTransaction,
    RevalidateAllTransactions,
    ValidateAllTransactions,
    SendShaAtomicSwapTransaction,
    CreateBurnTransaction,
    ClaimShaAtomicSwapTransaction,
    ClaimHtlcRefundTransaction,
    SignMessage,
    StreamTransactionEvents,
    ImportTransactions,
    GetAllCompletedTransactions,
    GetAllCompletedTransactionsStream,
    GetPaymentByReference,
    GetFeeEstimate,
    GetFeePerGramStats,
    ReplaceByFee,
    UserPayForFee,
    RegisterValidatorNode,
    SubmitValidatorNodeExit,
    GetBurnClaimProof,
    RescanWallet,
    ScanAndImportUtxos,
    DebugTransaction,
    ValidateTransaction,
    ValidateOutputs,
}

impl WalletGrpcMethod {
    /// All the wallet GRPC methods as a fixed array
    pub const ALL_VARIANTS: [WalletGrpcMethod; 50] = [
        WalletGrpcMethod::GetVersion,
        WalletGrpcMethod::GetState,
        WalletGrpcMethod::CheckConnectivity,
        WalletGrpcMethod::CheckForUpdates,
        WalletGrpcMethod::Identify,
        WalletGrpcMethod::GetAddress,
        WalletGrpcMethod::GetPaymentIdAddress,
        WalletGrpcMethod::GetCompleteAddress,
        WalletGrpcMethod::PrepareOneSidedTransactionForSigning,
        WalletGrpcMethod::PrepareDepositMultisigTransaction,
        WalletGrpcMethod::PrepareWithdrawMultisigTransaction,
        WalletGrpcMethod::BroadcastSignedOneSidedTransaction,
        WalletGrpcMethod::Transfer,
        WalletGrpcMethod::RangeLimitedCoinJoin,
        WalletGrpcMethod::GetTransactionInfo,
        WalletGrpcMethod::GetCompletedTransactions,
        WalletGrpcMethod::GetBlockHeightTransactions,
        WalletGrpcMethod::GetTransactionPayRefs,
        WalletGrpcMethod::GetBalance,
        WalletGrpcMethod::GetUnspentAmounts,
        WalletGrpcMethod::CoinSplit,
        WalletGrpcMethod::CoinHistogram,
        WalletGrpcMethod::ImportUtxos,
        WalletGrpcMethod::GetNetworkStatus,
        WalletGrpcMethod::GetConnectedHttpPeer,
        WalletGrpcMethod::CancelTransaction,
        WalletGrpcMethod::RevalidateAllTransactions,
        WalletGrpcMethod::ValidateAllTransactions,
        WalletGrpcMethod::SendShaAtomicSwapTransaction,
        WalletGrpcMethod::CreateBurnTransaction,
        WalletGrpcMethod::ClaimShaAtomicSwapTransaction,
        WalletGrpcMethod::ClaimHtlcRefundTransaction,
        WalletGrpcMethod::SignMessage,
        WalletGrpcMethod::StreamTransactionEvents,
        WalletGrpcMethod::ImportTransactions,
        WalletGrpcMethod::GetAllCompletedTransactions,
        WalletGrpcMethod::GetAllCompletedTransactionsStream,
        WalletGrpcMethod::GetPaymentByReference,
        WalletGrpcMethod::GetFeeEstimate,
        WalletGrpcMethod::GetFeePerGramStats,
        WalletGrpcMethod::ReplaceByFee,
        WalletGrpcMethod::UserPayForFee,
        WalletGrpcMethod::RegisterValidatorNode,
        WalletGrpcMethod::SubmitValidatorNodeExit,
        WalletGrpcMethod::GetBurnClaimProof,
        WalletGrpcMethod::RescanWallet,
        WalletGrpcMethod::ScanAndImportUtxos,
        WalletGrpcMethod::DebugTransaction,
        WalletGrpcMethod::ValidateTransaction,
        WalletGrpcMethod::ValidateOutputs,
    ];
}

impl IntoIterator for WalletGrpcMethod {
    type IntoIter = std::array::IntoIter<WalletGrpcMethod, 50>;
    type Item = WalletGrpcMethod;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(Self::ALL_VARIANTS)
    }
}

impl FromStr for WalletGrpcMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let binding = s.to_string().replace('\"', "");
        match binding.as_str() {
            "get_version" => Ok(WalletGrpcMethod::GetVersion),
            "get_state" => Ok(WalletGrpcMethod::GetState),
            "check_connectivity" => Ok(WalletGrpcMethod::CheckConnectivity),
            "check_for_updates" => Ok(WalletGrpcMethod::CheckForUpdates),
            "identify" => Ok(WalletGrpcMethod::Identify),
            "get_address" => Ok(WalletGrpcMethod::GetAddress),
            "get_payment_id_address" => Ok(WalletGrpcMethod::GetPaymentIdAddress),
            "get_complete_address" => Ok(WalletGrpcMethod::GetCompleteAddress),
            "prepare_one_sided_transaction_for_signing" => Ok(WalletGrpcMethod::PrepareOneSidedTransactionForSigning),
            "prepare_deposit_multisig_transaction" => Ok(WalletGrpcMethod::PrepareDepositMultisigTransaction),
            "prepare_withdraw_multisig_transaction" => Ok(WalletGrpcMethod::PrepareWithdrawMultisigTransaction),
            "broadcast_signed_one_sided_transaction" => Ok(WalletGrpcMethod::BroadcastSignedOneSidedTransaction),
            "transfer" => Ok(WalletGrpcMethod::Transfer),
            "range_limited_coin_join" => Ok(WalletGrpcMethod::RangeLimitedCoinJoin),
            "get_transaction_info" => Ok(WalletGrpcMethod::GetTransactionInfo),
            "get_completed_transactions" => Ok(WalletGrpcMethod::GetCompletedTransactions),
            "get_block_height_transactions" => Ok(WalletGrpcMethod::GetBlockHeightTransactions),
            "get_transaction_pay_refs" => Ok(WalletGrpcMethod::GetTransactionPayRefs),
            "get_balance" => Ok(WalletGrpcMethod::GetBalance),
            "get_unspent_amounts" => Ok(WalletGrpcMethod::GetUnspentAmounts),
            "coin_split" => Ok(WalletGrpcMethod::CoinSplit),
            "coin_histogram" => Ok(WalletGrpcMethod::CoinHistogram),
            "import_utxos" => Ok(WalletGrpcMethod::ImportUtxos),
            "get_network_status" => Ok(WalletGrpcMethod::GetNetworkStatus),
            "get_connected_http_peer" => Ok(WalletGrpcMethod::GetConnectedHttpPeer),
            "cancel_transaction" => Ok(WalletGrpcMethod::CancelTransaction),
            "revalidate_all_transactions" => Ok(WalletGrpcMethod::RevalidateAllTransactions),
            "validate_all_transactions" => Ok(WalletGrpcMethod::ValidateAllTransactions),
            "send_sha_atomic_swap_transaction" => Ok(WalletGrpcMethod::SendShaAtomicSwapTransaction),
            "create_burn_transaction" => Ok(WalletGrpcMethod::CreateBurnTransaction),
            "claim_sha_atomic_swap_transaction" => Ok(WalletGrpcMethod::ClaimShaAtomicSwapTransaction),
            "claim_htlc_refund_transaction" => Ok(WalletGrpcMethod::ClaimHtlcRefundTransaction),
            "sign_message" => Ok(WalletGrpcMethod::SignMessage),
            "stream_transaction_events" => Ok(WalletGrpcMethod::StreamTransactionEvents),
            "import_transactions" => Ok(WalletGrpcMethod::ImportTransactions),
            "get_all_completed_transactions" => Ok(WalletGrpcMethod::GetAllCompletedTransactions),
            "get_all_completed_transactions_stream" => Ok(WalletGrpcMethod::GetAllCompletedTransactionsStream),
            "get_payment_by_reference" => Ok(WalletGrpcMethod::GetPaymentByReference),
            "get_fee_estimate" => Ok(WalletGrpcMethod::GetFeeEstimate),
            "get_fee_per_gram_stats" => Ok(WalletGrpcMethod::GetFeePerGramStats),
            "replace_by_fee" => Ok(WalletGrpcMethod::ReplaceByFee),
            "user_pay_for_fee" => Ok(WalletGrpcMethod::UserPayForFee),
            "register_validator_node" => Ok(WalletGrpcMethod::RegisterValidatorNode),
            "submit_validator_node_exit" => Ok(WalletGrpcMethod::SubmitValidatorNodeExit),
            "get_burn_claim_proof" => Ok(WalletGrpcMethod::GetBurnClaimProof),
            "rescan_wallet" => Ok(WalletGrpcMethod::RescanWallet),
            "scan_and_import_utxos" => Ok(WalletGrpcMethod::ScanAndImportUtxos),
            "debug_transaction" => Ok(WalletGrpcMethod::DebugTransaction),
            "validate_transaction" => Ok(WalletGrpcMethod::ValidateTransaction),
            "validate_outputs" => Ok(WalletGrpcMethod::ValidateOutputs),
            _ => Err(format!("'{s}' not supported")),
        }
    }
}

impl fmt::Display for WalletGrpcMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};

    use crate::grpc_method::WalletGrpcMethod;

    #[derive(Clone, Serialize, Deserialize, Debug)]
    struct TestConfig {
        name: String,
        inner_config: TestInnerConfig,
    }

    #[derive(Clone, Serialize, Deserialize, Debug)]
    struct TestInnerConfig {
        allow_methods: Vec<WalletGrpcMethod>,
    }

    #[test]
    fn it_deserializes_enums() {
        let config_str = r#"{
            "name": "wallet champion",
            "inner_config": {
                "allow_methods": [
                    "get_balance",
                    "get_version",
                    "identify"
                ]
            }
        }"#;
        let config = serde_json::from_str::<TestConfig>(config_str).unwrap();

        assert!(
            config
                .inner_config
                .allow_methods
                .contains(&WalletGrpcMethod::GetBalance)
        );
        assert!(
            config
                .inner_config
                .allow_methods
                .contains(&WalletGrpcMethod::GetVersion)
        );
        assert!(!config.inner_config.allow_methods.contains(&WalletGrpcMethod::Transfer));
        assert!(config.inner_config.allow_methods.contains(&WalletGrpcMethod::Identify));
        assert!(!config.inner_config.allow_methods.contains(&WalletGrpcMethod::CoinSplit));
    }

    #[test]
    fn wallet_grpc_method_into_iter_is_exhaustive() {
        let mut count = 0;
        for method in &WalletGrpcMethod::ALL_VARIANTS {
            match method {
                WalletGrpcMethod::GetVersion => count += 1,
                WalletGrpcMethod::GetState => count += 1,
                WalletGrpcMethod::CheckConnectivity => count += 1,
                WalletGrpcMethod::CheckForUpdates => count += 1,
                WalletGrpcMethod::Identify => count += 1,
                WalletGrpcMethod::GetAddress => count += 1,
                WalletGrpcMethod::GetPaymentIdAddress => count += 1,
                WalletGrpcMethod::GetCompleteAddress => count += 1,
                WalletGrpcMethod::PrepareOneSidedTransactionForSigning => count += 1,
                WalletGrpcMethod::PrepareDepositMultisigTransaction => count += 1,
                WalletGrpcMethod::PrepareWithdrawMultisigTransaction => count += 1,
                WalletGrpcMethod::BroadcastSignedOneSidedTransaction => count += 1,
                WalletGrpcMethod::Transfer => count += 1,
                WalletGrpcMethod::RangeLimitedCoinJoin => count += 1,
                WalletGrpcMethod::GetTransactionInfo => count += 1,
                WalletGrpcMethod::GetCompletedTransactions => count += 1,
                WalletGrpcMethod::GetBlockHeightTransactions => count += 1,
                WalletGrpcMethod::GetTransactionPayRefs => count += 1,
                WalletGrpcMethod::GetBalance => count += 1,
                WalletGrpcMethod::GetUnspentAmounts => count += 1,
                WalletGrpcMethod::CoinSplit => count += 1,
                WalletGrpcMethod::CoinHistogram => count += 1,
                WalletGrpcMethod::ImportUtxos => count += 1,
                WalletGrpcMethod::GetNetworkStatus => count += 1,
                WalletGrpcMethod::GetConnectedHttpPeer => count += 1,
                WalletGrpcMethod::CancelTransaction => count += 1,
                WalletGrpcMethod::RevalidateAllTransactions => count += 1,
                WalletGrpcMethod::ValidateAllTransactions => count += 1,
                WalletGrpcMethod::SendShaAtomicSwapTransaction => count += 1,
                WalletGrpcMethod::CreateBurnTransaction => count += 1,
                WalletGrpcMethod::ClaimShaAtomicSwapTransaction => count += 1,
                WalletGrpcMethod::ClaimHtlcRefundTransaction => count += 1,
                WalletGrpcMethod::SignMessage => count += 1,
                WalletGrpcMethod::StreamTransactionEvents => count += 1,
                WalletGrpcMethod::ImportTransactions => count += 1,
                WalletGrpcMethod::GetAllCompletedTransactions => count += 1,
                WalletGrpcMethod::GetAllCompletedTransactionsStream => count += 1,
                WalletGrpcMethod::GetPaymentByReference => count += 1,
                WalletGrpcMethod::GetFeeEstimate => count += 1,
                WalletGrpcMethod::GetFeePerGramStats => count += 1,
                WalletGrpcMethod::ReplaceByFee => count += 1,
                WalletGrpcMethod::UserPayForFee => count += 1,
                WalletGrpcMethod::RegisterValidatorNode => count += 1,
                WalletGrpcMethod::SubmitValidatorNodeExit => count += 1,
                WalletGrpcMethod::GetBurnClaimProof => count += 1,
                WalletGrpcMethod::RescanWallet => count += 1,
                WalletGrpcMethod::ScanAndImportUtxos => count += 1,
                WalletGrpcMethod::DebugTransaction => count += 1,
                WalletGrpcMethod::ValidateTransaction => count += 1,
                WalletGrpcMethod::ValidateOutputs => count += 1,
            }
        }
        assert_eq!(count, WalletGrpcMethod::ALL_VARIANTS.len());
    }

    #[test]
    fn all_variants_are_listed_once() {
        let mut seen = Vec::with_capacity(WalletGrpcMethod::ALL_VARIANTS.len());
        for method in &WalletGrpcMethod::ALL_VARIANTS {
            assert!(!seen.contains(method), "'{method}' is listed more than once");
            seen.push(*method);
        }
    }

    #[test]
    fn it_converts_config_strings_to_enum() {
        assert_eq!(
            WalletGrpcMethod::from_str("get_balance").unwrap(),
            WalletGrpcMethod::GetBalance
        );
        assert_eq!(
            WalletGrpcMethod::from_str("transfer").unwrap(),
            WalletGrpcMethod::Transfer
        );
        assert!(WalletGrpcMethod::from_str("unknown_wallet_method").is_err());
    }

    #[test]
    fn it_converts_from_serde_json_str_to_enum() {
        for method in &WalletGrpcMethod::ALL_VARIANTS {
            let method_str = serde_json::to_string(&method).unwrap();
            let method_from_str = WalletGrpcMethod::from_str(&method_str).unwrap();
            assert_eq!(method, &method_from_str);
        }
    }
}
