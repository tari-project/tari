// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::{digest::consts::U32, Blake2b};
use digest::consts::U64;
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, DomainSeparation},
};

use crate::{domains::ValidatorNodeMerkleHashDomain, DomainSeparatedBorshHasher, ValidatorNodeHashDomain};

hash_domain!(TariDanConsensusHashDomain, "com.tari.consensus", 0);

pub type TariDomainHasher<M, OutSize> = DomainSeparatedBorshHasher<M, Blake2b<OutSize>>;
pub type TariConsensusHasher = TariDomainHasher<TariDanConsensusHashDomain, U32>;

pub fn tari_hasher64<M: DomainSeparation>(label: &'static str) -> TariDomainHasher<M, U64> {
    TariDomainHasher::<M, U64>::new_with_label(label)
}

pub fn tari_hasher32<M: DomainSeparation>(label: &'static str) -> TariDomainHasher<M, U32> {
    TariDomainHasher::<M, U32>::new_with_label(label)
}

pub fn tari_consensus_hasher(label: &'static str) -> TariConsensusHasher {
    TariConsensusHasher::new_with_label(label)
}

pub fn validator_registration_hasher() -> TariDomainHasher<ValidatorNodeHashDomain, U64> {
    tari_hasher64("registration")
}

pub fn block_hasher() -> TariConsensusHasher {
    tari_consensus_hasher("Block")
}

pub fn block_metadata_hasher() -> TariConsensusHasher {
    tari_consensus_hasher("BlockMetadata")
}

pub fn command_hasher() -> TariConsensusHasher {
    tari_consensus_hasher("Command")
}

pub fn proposal_vote_signature_hasher() -> TariConsensusHasher {
    tari_consensus_hasher("VoteSignature")
}

pub fn timeout_vote_signature_hasher() -> TariConsensusHasher {
    tari_consensus_hasher("TimeoutVoteSignature")
}

pub type ValidatorNodeBmtHasherBlake2b = DomainSeparatedHasher<Blake2b<U32>, ValidatorNodeMerkleHashDomain>;
