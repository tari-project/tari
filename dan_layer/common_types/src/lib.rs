// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod proto;
pub mod storage;

mod hash;
mod template_id;

pub use hash::Hash;
use tari_common::hashing_domain::HashingDomain;
pub use template_id::TemplateId;

/// The DAN layer domain separated hashing domain
/// Usage:
///   let hash = dan_layer_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn dan_layer_hash_domain() -> HashingDomain {
    HashingDomain::new("dan_layer")
}
