// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod wallet_grpc_server;

use tari_app_grpc::tari_rpc::TransactionEvent;
use tari_utilities::hex::Hex;
use tari_wallet::transaction_service::storage::models::{
    CompletedTransaction,
    InboundTransaction,
    OutboundTransaction,
};

pub use self::wallet_grpc_server::*;

pub enum TransactionWrapper {
    Completed(CompletedTransaction),
    Outbound(OutboundTransaction),
    Inbound(InboundTransaction),
}

pub fn convert_to_transaction_event(event: String, source: TransactionWrapper) -> TransactionEvent {
    match source {
        TransactionWrapper::Completed(completed) => TransactionEvent {
            event,
            tx_id: completed.tx_id.to_string(),
            source_pk: completed.source_public_key.to_hex().into_bytes(),
            dest_pk: completed.destination_public_key.to_hex().into_bytes(),
            status: completed.status.to_string(),
            direction: completed.direction.to_string(),
            amount: completed.amount.as_u64(),
            message: completed.message.to_string(),
            is_coinbase: completed.is_coinbase(),
        },
        TransactionWrapper::Outbound(outbound) => TransactionEvent {
            event,
            tx_id: outbound.tx_id.to_string(),
            source_pk: vec![],
            dest_pk: outbound.destination_public_key.to_hex().into_bytes(),
            status: outbound.status.to_string(),
            direction: "outbound".to_string(),
            amount: outbound.amount.as_u64(),
            message: outbound.message,
            is_coinbase: false,
        },
        TransactionWrapper::Inbound(inbound) => TransactionEvent {
            event,
            tx_id: inbound.tx_id.to_string(),
            source_pk: inbound.source_public_key.to_hex().into_bytes(),
            dest_pk: vec![],
            status: inbound.status.to_string(),
            direction: "inbound".to_string(),
            amount: inbound.amount.as_u64(),
            message: inbound.message.clone(),
            /// The coinbase are technically Inbound.
            /// To determine whether a transaction is coinbase
            /// we will check whether the message contains `Coinbase`.
            is_coinbase: if inbound.message.to_lowercase().contains("coinbase") {
                true
            } else {
                false
            },
        },
    }
}
