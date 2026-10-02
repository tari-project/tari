// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use blake2::{Blake2b, digest::consts::U32};
use digest::consts::U64;
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, DomainSeparation},
};

use crate::{DomainSeparatedBorshHasher, ValidatorNodeHashDomain, domains::ValidatorNodeMerkleHashDomain};

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

/// Hasher for the message a validator node signs to register.
///
/// The network byte is chained first so that a registration signed for one network can never verify on another, and
/// the label is distinct from [`validator_exit_hasher`] so that a registration signature can never be presented as an
/// exit signature (or vice versa), even if the remaining fields happen to line up.
pub fn validator_registration_hasher(network: u8) -> TariDomainHasher<ValidatorNodeHashDomain, U64> {
    tari_hasher64("vn_registration").chain(&network)
}

/// Hasher for the message a validator node signs to exit.
///
/// The network byte is chained first so that an exit signed for one network can never verify on another, and the
/// label is distinct from [`validator_registration_hasher`] so that the two signature purposes cannot be confused.
pub fn validator_exit_hasher(network: u8) -> TariDomainHasher<ValidatorNodeHashDomain, U64> {
    tari_hasher64("vn_exit").chain(&network)
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
