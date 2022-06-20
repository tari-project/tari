// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod error;
mod wallet_grpc_client;
use std::convert::TryFrom;

use futures::{Future, Stream};
use serde::Serialize;
use tari_app_grpc::tari_rpc::{GetBalanceResponse, GetIdentityResponse, TransactionEvent};
use thiserror::Error;
pub use wallet_grpc_client::*;

#[derive(Debug, Clone, Serialize)]
pub struct WalletTransaction {
    pub event: String,
    pub tx_id: String,
    pub source_pk: Vec<u8>,
    pub dest_pk: Vec<u8>,
    pub status: String,
    pub direction: String,
    pub amount: u64,
    pub message: String,
    pub is_coinbase: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletIdentity {
    public_key: Vec<u8>,
    public_address: String,
    node_id: Vec<u8>,
}

pub struct WalletBalance {
    available_balance: u64,
    pending_incoming_balance: u64,
    pending_outgoing_balance: u64,
}

impl TryFrom<TransactionEvent> for WalletTransaction {
    type Error = String;

    fn try_from(value: TransactionEvent) -> Result<Self, Self::Error> {
        match value.event.as_str() {
            "not_supported" => Err("event is not supported.".to_string()),
            _ => Ok(WalletTransaction {
                event: value.event,
                tx_id: value.tx_id,
                source_pk: value.source_pk,
                dest_pk: value.dest_pk,
                status: value.status,
                direction: value.direction,
                amount: value.amount,
                message: value.message,
                is_coinbase: value.is_coinbase,
            }),
        }
    }
}

impl From<GetIdentityResponse> for WalletIdentity {
    fn from(value: GetIdentityResponse) -> WalletIdentity {
        WalletIdentity {
            public_key: value.public_key,
            public_address: value.public_address,
            node_id: value.node_id,
        }
    }
}

impl From<GetBalanceResponse> for WalletBalance {
    fn from(value: GetBalanceResponse) -> WalletBalance {
        WalletBalance {
            available_balance: value.available_balance,
            pending_incoming_balance: value.pending_incoming_balance,
            pending_outgoing_balance: value.pending_outgoing_balance,
        }
    }
}
