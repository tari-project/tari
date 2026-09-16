// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::Blake2b;
use digest::consts::U32;
use tari_crypto::hashing::DomainSeparatedHasher;

use crate::KernelMmrHashDomain;

pub type KernelMmrHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, KernelMmrHashDomain>;
