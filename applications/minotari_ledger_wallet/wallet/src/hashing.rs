// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use alloc::format;
use core::marker::PhantomData;

use borsh::{io, io::Write, BorshSerialize};
use digest::Digest;

use crate::crypto::hashing::DomainSeparation;
pub struct DomainSeparatedConsensusHasher<M, D> {
    hasher: DomainSeparatedBorshHasher<M, D>,
}
use digest::Output;

impl<M: DomainSeparation, D: Digest> DomainSeparatedConsensusHasher<M, D>
where D: Default
{
    /// `network` is the network byte (`Network::as_byte` on the host). It is a `u8` so that callers holding a wider
    /// wire value must convert it explicitly (and reject out-of-range values) rather than have it silently truncated.
    pub fn new(label: &'static str, network: u8) -> Self {
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

pub struct DomainSeparatedHash<D: Digest> {
    pub output: Output<D>,
}

impl<D: Digest> DomainSeparatedHash<D> {
    pub fn new(output: Output<D>) -> Self {
        Self { output }
    }
}

impl<D: Digest> AsRef<[u8]> for DomainSeparatedHash<D> {
    fn as_ref(&self) -> &[u8] {
        self.output.as_slice()
    }
}
