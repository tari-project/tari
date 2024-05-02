// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;
use core::marker::PhantomData;

use borsh::{io, io::Write, BorshSerialize};
use digest::Digest;
use tari_crypto::hashing::DomainSeparation;

pub struct DomainSeparatedConsensusHasher<M, D> {
    hasher: DomainSeparatedBorshHasher<M, D>,
}

impl<M: DomainSeparation, D: Digest> DomainSeparatedConsensusHasher<M, D>
where D: Default
{
    pub fn new(label: &'static str, network: u64) -> Self {
        let hasher = DomainSeparatedBorshHasher::<M, D>::new_with_label(&format!("{}.n{}", label, network));
        Self { hasher }
    }

    pub fn finalize(self) -> digest::Output<D> {
        self.hasher.finalize()
    }

    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        self.hasher.update_consensus_encode(data);
    }

    pub fn chain<T: BorshSerialize>(mut self, data: &T) -> Self {
        self.update_consensus_encode(data);
        self
    }
}

/// Domain separated borsh-encoding hasher.
pub struct DomainSeparatedBorshHasher<M, D> {
    writer: WriteHashWrapper<D>,
    _m: PhantomData<M>,
}

impl<D: Digest + Default, M: DomainSeparation> DomainSeparatedBorshHasher<M, D> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_label(label: &str) -> Self {
        let mut digest = D::default();
        M::add_domain_separation_tag(&mut digest, label);
        Self {
            writer: WriteHashWrapper(digest),
            _m: PhantomData,
        }
    }

    pub fn finalize(self) -> digest::Output<D> {
        self.writer.0.finalize()
    }

    pub fn update_consensus_encode<T: BorshSerialize>(&mut self, data: &T) {
        BorshSerialize::serialize(data, &mut self.writer)
            .expect("Incorrect implementation of BorshSerialize encountered. Implementations MUST be infallible.");
    }
}

/// This private struct wraps a Digest and implements the Write trait to satisfy the consensus encoding trait.
/// Do not use the DomainSeparatedHasher with this.
#[derive(Clone)]
struct WriteHashWrapper<D>(D);

impl<D: Digest> Write for WriteHashWrapper<D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
