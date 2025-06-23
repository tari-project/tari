// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Cryptographic primitives for lightweight wallets
//! 
//! This module provides our own implementations of cryptographic primitives
//! to avoid dependencies on tari-crypto and tari-utilities.

pub mod domain_separated_hasher;
pub mod keys;
pub mod hash_domain;

pub use domain_separated_hasher::DomainSeparatedHasher;
pub use keys::{RistrettoSecretKey, RistrettoPublicKey};
pub use hash_domain::KeyManagerDomain; 