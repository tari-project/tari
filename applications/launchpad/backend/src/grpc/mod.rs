// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

mod base_node_grpc_client;
mod error;
mod model;
mod progress;
mod wallet_grpc_client;
use std::convert::TryFrom;

pub use base_node_grpc_client::*;
use futures::{Future, Stream};
use log::{error, info};
pub use model::*;
pub use progress::*;
use serde::Serialize;
use tari_app_grpc::tari_rpc::{GetBalanceResponse, GetIdentityResponse, TransactionEvent};
use tari_common_types::{emoji::EmojiId, types::PublicKey};
use thiserror::Error;
pub use wallet_grpc_client::*;
