// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod proto;
pub mod storage;

mod template_id;

use tari_common::hashing_domain::HashingDomain;
pub use template_id::TemplateId;

/// The DAN layer domain separated hashing domain
/// Usage:
///   let hash = DAN_LAYER_HASH_DOMAIN.digest::<Blake256>(b"my secret");
///   etc.
pub const DAN_LAYER_HASH_DOMAIN: HashingDomain = HashingDomain::new("tari_project.dan_layer");
