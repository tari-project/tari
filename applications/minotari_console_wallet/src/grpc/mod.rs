// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod wallet_grpc_server;

use minotari_app_grpc::tari_rpc::TransactionEvent;
use minotari_wallet::transaction_service::storage::models::{
    CompletedTransaction,
    InboundTransaction,
    OutboundTransaction,
};

pub use self::wallet_grpc_server::*;

pub enum TransactionWrapper {
    Completed(Box<CompletedTransaction>),
    Outbound(Box<OutboundTransaction>),
    Inbound(Box<InboundTransaction>),
}

pub fn convert_to_transaction_event(event: String, source: TransactionWrapper) -> TransactionEvent {
    match source {
        TransactionWrapper::Completed(completed) => TransactionEvent {
            event,
            tx_id: completed.tx_id.to_string(),
            source_address: completed.source_address.to_vec(),
            dest_address: completed.destination_address.to_vec(),
            status: completed.status.to_string(),
            direction: completed.direction.to_string(),
            amount: completed.amount.as_u64(),
            message: completed.message.to_string(),
            payment_id: completed.payment_id.map(|id| id.to_bytes()).unwrap_or_default(),
        },
        TransactionWrapper::Outbound(outbound) => TransactionEvent {
            event,
            tx_id: outbound.tx_id.to_string(),
            source_address: vec![],
            dest_address: outbound.destination_address.to_vec(),
            status: outbound.status.to_string(),
            direction: "outbound".to_string(),
            amount: outbound.amount.as_u64(),
            message: outbound.message,
            payment_id: vec![],
        },
        TransactionWrapper::Inbound(inbound) => TransactionEvent {
            event,
            tx_id: inbound.tx_id.to_string(),
            source_address: inbound.source_address.to_vec(),
            dest_address: vec![],
            status: inbound.status.to_string(),
            direction: "inbound".to_string(),
            amount: inbound.amount.as_u64(),
            message: inbound.message.clone(),
            payment_id: vec![],
        },
    }
}
