// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::Blake2b;
use digest::consts::U32;
use tari_crypto::hashing::DomainSeparatedHasher;

use crate::{InputMmrHashDomain, KernelMmrHashDomain};

pub type KernelMmrHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, KernelMmrHashDomain>;
/// Hasher of the block input MMR and the block output MMRs (`header.block_output_mr`).
pub type InputMmrHasherBlake256 = DomainSeparatedHasher<Blake2b<U32>, InputMmrHashDomain>;
