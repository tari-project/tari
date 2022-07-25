// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common::hashing_domain::HashingDomain;

pub mod flow;
pub mod function_definitions;
pub mod instructions;
pub mod models;
pub mod state;
pub mod wasm;

pub mod compile;
pub mod crypto;
pub mod env;
pub mod instruction;
pub mod package;
pub mod traits;

/// The DAN layer engine domain separated hashing domain
/// Usage:
///   let hash = dan_layer_engine_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn dan_layer_engine_hash_domain() -> HashingDomain {
    HashingDomain::new("dan_layer.engine")
}
