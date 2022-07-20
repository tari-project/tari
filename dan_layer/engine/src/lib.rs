// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_common::hashing_domain::HashingDomain;

pub mod flow;
pub mod function_definitions;
pub mod instructions;
pub mod models;
pub mod state;
pub mod wasm;

/// The DAN layer engine domain separated hashing domain
/// Usage:
///   let hash = DAN_LAYER_ENGINE_HASH_DOMAIN.digest::<Blake256>(b"my secret");
///   etc.
pub const DAN_LAYER_ENGINE_HASH_DOMAIN: HashingDomain = HashingDomain::new("tari_project.dan_layer.engine");
