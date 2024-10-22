// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use hickory_client::proto::rr::dnssec::{public_key::Rsa, TrustAnchor};
pub use webpki_roots::TLS_SERVER_ROOTS;

#[inline]
pub fn default_trust_anchor() -> TrustAnchor {
    // This was copied from the trust-dns crate.
    const ROOT_ANCHOR_ORIG: &[u8] = include_bytes!("19036.rsa");
    // This was generated from the `.` root domain in 10/2020.
    const ROOT_ANCHOR_CURRENT: &[u8] = include_bytes!("20326.rsa");

    let mut anchor = TrustAnchor::new();
    anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_ORIG).expect("Invalid ROOT_ANCHOR_ORIG"));
    anchor.insert_trust_anchor(&Rsa::from_public_bytes(ROOT_ANCHOR_CURRENT).expect("Invalid ROOT_ANCHOR_CURRENT"));
    anchor
}
