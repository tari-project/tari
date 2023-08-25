// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # MinoTari Ledger Wallet - Hashing

use core::marker::PhantomData;

use blake2::Blake2b;
use borsh::{
    maybestd::io::{Result as BorshResult, Write},
    BorshSerialize,
};
use digest::{consts::U64, Digest};
use tari_crypto::hashing::DomainSeparation;

/// Domain separated consensus hasher
pub struct DomainSeparatedConsensusHasher<M>(PhantomData<M>);

impl<M: DomainSeparation> DomainSeparatedConsensusHasher<M> {
    /// Create a new hasher with the given label
    pub fn new(label: &'static str) -> ConsensusHasher<Blake2b<U64>> {
        let mut digest = Blake2b::<U64>::new();
        M::add_domain_separation_tag(&mut digest, label);
        ConsensusHasher::from_digest(digest)
    }
}

/// Consensus hasher
#[derive(Clone)]
pub struct ConsensusHasher<D> {
    writer: WriteHashWrapper<D>,
}

impl<D: Digest> ConsensusHasher<D> {
    fn from_digest(digest: D) -> Self {
        Self {
            writer: WriteHashWrapper(digest),
        }
    }
}

impl<D> ConsensusHasher<D>
where D: Digest<OutputSize = U64>
{
    /// Finalize the hasher and return the hash
    pub fn finalize(self) -> [u8; 64] {
        self.writer.0.finalize().into()
    }

    /// Update the hasher with the given data
    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        BorshSerialize::serialize(data, &mut self.writer)
            .expect("Incorrect implementation of BorshSerialize encountered. Implementations MUST be infallible.");
    }

    /// Update the hasher with the given data
    pub fn chain<T: BorshSerialize>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

#[derive(Clone)]
struct WriteHashWrapper<D>(D);

impl<D: Digest> Write for WriteHashWrapper<D> {
    fn write(&mut self, buf: &[u8]) -> BorshResult<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> BorshResult<()> {
        Ok(())
    }
}
