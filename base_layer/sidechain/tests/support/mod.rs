// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::de::DeserializeOwned;
use tari_common_types::types::{CompressedPublicKey, FixedHash, PrivateKey};
use tari_crypto::keys::SecretKey;
use tari_sidechain::{
    ChainLink,
    CommitProofElement,
    DecodedValidatorBlockSignature,
    ProposalVoteMessage,
    QcSignatures,
    QuorumCertificate,
    QuorumDecision,
    ShardGroup,
    ShardGroupAccumulatedData,
    SidechainBlockCommitProof,
    SidechainBlockHeader,
    SignerBitmap,
    SigningCommittee,
    ValidatorBlockSignature,
};
use tari_utilities::ByteArray;

/// The protocol version of the blocks in `commit_proof.json`.
pub const PROTOCOL_VERSION: u32 = 0;

pub fn load_fixture<T: DeserializeOwned>(name: &str) -> T {
    let path = format!("tests/fixtures/{name}");
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}

/// The committee that signed the certificates in `commit_proof.json`: test validators 1 to 4, each with power 1.
pub fn fixture_committee() -> TestCommittee {
    test_committee(1..=4, |_| 1)
}

/// The proof in `commit_proof.json`.
pub fn fixture_proof() -> SidechainBlockCommitProof {
    signed_proof(&fixture_committee(), 1..=4, test_header(None))
}

/// The secret key of test validator `i`.
pub fn validator_secret(i: u8) -> PrivateKey {
    PrivateKey::from_uniform_bytes(&[i; 64]).unwrap()
}

pub fn validator_public_key(i: u8) -> CompressedPublicKey {
    CompressedPublicKey::from_secret_key(&validator_secret(i))
}

/// A committee of test validators `validators`, where validator `i` holds `power_of(i)`.
pub fn test_committee(validators: impl IntoIterator<Item = u8>, power_of: impl Fn(u8) -> u64) -> TestCommittee {
    TestCommittee::new(validators.into_iter().map(|i| (validator_public_key(i), power_of(i))))
}

/// A header at [`PROTOCOL_VERSION`] proposed by test validator 1.
pub fn test_header(transaction_merkle_root: Option<FixedHash>) -> SidechainBlockHeader {
    SidechainBlockHeader {
        network: 16,
        protocol_version: PROTOCOL_VERSION,
        parent_id: FixedHash::from([0x01; 32]),
        justify_id: FixedHash::from([0x02; 32]),
        height: 20,
        epoch: 1,
        epoch_hash: FixedHash::from([0x03; 32]),
        shard_group: ShardGroup {
            start: 1,
            end_inclusive: 256,
        },
        proposed_by: validator_public_key(1),
        state_merkle_root: FixedHash::from([0x04; 32]),
        command_merkle_root: FixedHash::from([0x05; 32]),
        transaction_merkle_root,
        signature: ValidatorBlockSignature::default(),
        accumulated_data: ShardGroupAccumulatedData::default(),
        metadata_hash: FixedHash::from([0x06; 32]),
    }
}

/// A proof that `header` committed, by a 3-chain of certificates that test validators `signers` of `committee` sign.
pub fn signed_proof(
    committee: &TestCommittee,
    signers: impl IntoIterator<Item = u8>,
    header: SidechainBlockHeader,
) -> SidechainBlockCommitProof {
    let mut signers = signers
        .into_iter()
        .map(|i| {
            let index = committee
                .index_of(&validator_public_key(i))
                .expect("signer must be a committee member");
            (index, validator_secret(i))
        })
        .collect::<Vec<_>>();
    signers.sort_by_key(|(index, _)| *index);
    let bitmap = SignerBitmap::from_indices(committee.len(), signers.iter().map(|(index, _)| *index)).unwrap();

    let certify = |header_hash: FixedHash, parent_id: FixedHash, height: u64| {
        let block_id = ChainLink { header_hash, parent_id }.calc_block_id();
        let message = ProposalVoteMessage::new(
            header.protocol_version,
            &block_id,
            QuorumDecision::Accept,
            header.epoch,
            height,
        )
        .calculate_hash();
        let signatures = signers
            .iter()
            .map(|(_, secret)| {
                let nonce_seed = [secret.as_bytes(), message.as_slice()].concat();
                let nonce = PrivateKey::from_uniform_bytes(&nonce_seed).unwrap();
                let signature =
                    DecodedValidatorBlockSignature::sign_with_nonce_and_message(secret, nonce, message).unwrap();
                ValidatorBlockSignature::new_from_schnorr(signature)
            })
            .collect();
        QuorumCertificate {
            header_hash,
            parent_id,
            protocol_version: header.protocol_version,
            epoch: header.epoch,
            height,
            signatures: QcSignatures::Schnorr {
                signers: bitmap.clone(),
                signatures,
            },
            decision: QuorumDecision::Accept,
        }
    };

    let block_id = header.calculate_block_id();
    let child_hash = FixedHash::from([0xc1; 32]);
    let child_id = ChainLink {
        header_hash: child_hash,
        parent_id: block_id,
    }
    .calc_block_id();
    let grandchild_hash = FixedHash::from([0xc2; 32]);

    let proof_elements = vec![
        CommitProofElement::QuorumCertificate(certify(grandchild_hash, child_id, header.height.saturating_add(2))),
        CommitProofElement::QuorumCertificate(certify(child_hash, block_id, header.height.saturating_add(1))),
        CommitProofElement::QuorumCertificate(certify(header.calculate_hash(), header.parent_id, header.height)),
    ];
    SidechainBlockCommitProof { header, proof_elements }
}

pub struct TestCommittee {
    members: Vec<(CompressedPublicKey, u64)>,
}

impl TestCommittee {
    /// A committee of `members`, sorted into ascending order of public key.
    pub fn new(members: impl IntoIterator<Item = (CompressedPublicKey, u64)>) -> Self {
        let mut members = members.into_iter().collect::<Vec<_>>();
        members.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
        Self { members }
    }

    /// A committee of `members` in the order given.
    pub fn in_order(members: Vec<(CompressedPublicKey, u64)>) -> Self {
        Self { members }
    }

    pub fn index_of(&self, public_key: &CompressedPublicKey) -> Option<usize> {
        self.members.iter().position(|(pk, _)| pk == public_key)
    }

    pub fn public_keys(&self) -> impl Iterator<Item = &CompressedPublicKey> {
        self.members.iter().map(|(pk, _)| pk)
    }
}

impl SigningCommittee for TestCommittee {
    fn len(&self) -> usize {
        self.members.len()
    }

    fn member(&self, index: usize) -> Option<(&[u8], u64)> {
        self.members.get(index).map(|(pk, power)| (pk.as_bytes(), *power))
    }
}
