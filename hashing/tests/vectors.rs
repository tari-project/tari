// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Known-answer vectors for every hash domain and public hasher in `tari_hashing`.
//!
//! `tests/vectors.json` is a cross-repo artefact: the Ledger app (which carries its own copy of the domains it uses,
//! see `minotari_ledger_wallet_comms_testing`) and Ootle check their copies against it. Any change to a domain
//! string, version, label or hasher construction changes consensus or wallet compatibility and must show up here.
//!
//! Domain tags: `tag_hex` is exactly the bytes `DomainSeparation::add_domain_separation_tag(digest, "")` absorbs:
//! `u64_le(len(tag)) || "{domain}.v{version}"`. With a non-empty label the tag is `"{domain}.v{version}.{label}"`.
//!
//! Hashers: `digest_hex` is the hasher (with the given label) after absorbing `input_hex`:
//! - `"absorb": "borsh"` (`DomainSeparatedBorshHasher` and the `layer2` helpers): the input is chained as a borsh `[u8;
//!   32]`, i.e. the raw bytes with no length prefix. Where `network` is present, the hasher has already chained that
//!   network byte (as borsh `u8`) before the input.
//! - `"absorb": "length_prefixed"` (`tari_crypto::hashing::DomainSeparatedHasher`): the input is absorbed with
//!   `DomainSeparatedHasher::update`, which prepends `u64_le(len(input))`.
//!
//! To regenerate after an intentional change: `TARI_HASHING_REGENERATE_VECTORS=1 cargo test -p tari_hashing --test
//! vectors known_answer_vectors` (the variable must be exactly `1`). That run writes the file and then fails on
//! purpose; review the diff and re-run without the variable.

use std::{fs, path::PathBuf};

use blake2::Blake2b;
use digest::{
    Digest,
    consts::{U32, U64},
};
use serde::{Deserialize, Serialize};
use tari_crypto::hashing::{DomainSeparatedHasher, DomainSeparation};
use tari_hashing::{
    BlocksHashDomain,
    BulletRangeProofHashDomain,
    ConfidentialOutputHashDomain,
    DomainSeparatedBorshHasher,
    InputMmrHashDomain,
    KernelMmrHashDomain,
    KeyManagerDomain,
    KeyManagerTransactionsHashDomain,
    LedgerHashDomain,
    OfflineSigningPayloadHashDomain,
    PaymentReferenceHashDomain,
    TransactionHashDomain,
    TransactionSecureNonceKdfDomain,
    ValidatorNodeHashDomain,
    ValidatorNodeMerkleHashDomain,
    WalletHasher,
    WalletMessageSigningDomain,
    WalletOutputEncryptionKeysDomain,
    WalletOutputRewindKeysDomain,
    WalletOutputSpendingKeysDomain,
    hashers::{InputMmrHasherBlake256, KernelMmrHasherBlake256},
    layer2::{
        TariDanConsensusHashDomain,
        ValidatorNodeBmtHasherBlake2b,
        block_hasher,
        block_metadata_hasher,
        command_hasher,
        proposal_vote_signature_hasher,
        timeout_vote_signature_hasher,
        validator_exit_hasher,
        validator_registration_hasher,
    },
};

/// Network byte used for the network-bound hashers (`Network::MainNet.as_byte()`).
const NETWORK: u8 = 0x00;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Vectors {
    domains: Vec<DomainVector>,
    hashers: Vec<HasherVector>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct DomainVector {
    name: String,
    domain: String,
    version: u8,
    tag_hex: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct HasherVector {
    name: String,
    label: String,
    absorb: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    network: Option<u8>,
    input_hex: String,
    digest_hex: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn input() -> [u8; 32] {
    let mut input = [0u8; 32];
    for (i, b) in input.iter_mut().enumerate() {
        *b = u8::try_from(i).unwrap();
    }
    input
}

/// The tag bytes, built by hand from the domain string and version.
fn tag_bytes(domain: &str, version: u8) -> Vec<u8> {
    let tag = format!("{domain}.v{version}");
    let mut bytes = (tag.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(tag.as_bytes());
    bytes
}

fn domain_vector<M: DomainSeparation>(name: &str) -> DomainVector {
    let tag = tag_bytes(M::domain(), M::version());
    // The hand-built bytes must be exactly what the implementation absorbs.
    let mut hasher = Blake2b::<U32>::new();
    M::add_domain_separation_tag(&mut hasher, "");
    assert_eq!(hasher.finalize(), Blake2b::<U32>::digest(&tag), "{name}");
    DomainVector {
        name: name.to_string(),
        domain: M::domain().to_string(),
        version: M::version(),
        tag_hex: to_hex(&tag),
    }
}

/// For domains that are only reachable through a hasher type (the `hasher!` macro hides the domain type).
fn domain_vector_of_hasher<M: DomainSeparation>(
    name: &str,
    _hasher: &DomainSeparatedHasher<Blake2b<U64>, M>,
) -> DomainVector {
    let tag = tag_bytes(M::domain(), M::version());
    let empty = DomainSeparatedHasher::<Blake2b<U64>, M>::new().finalize();
    assert_eq!(empty.as_ref(), Blake2b::<U64>::digest(&tag).as_slice(), "{name}");
    DomainVector {
        name: name.to_string(),
        domain: M::domain().to_string(),
        version: M::version(),
        tag_hex: to_hex(&tag),
    }
}

fn borsh_vector(name: &str, label: &str, network: Option<u8>, digest: &[u8]) -> HasherVector {
    HasherVector {
        name: name.to_string(),
        label: label.to_string(),
        absorb: "borsh".to_string(),
        network,
        input_hex: to_hex(&input()),
        digest_hex: to_hex(digest),
    }
}

fn length_prefixed_vector(name: &str, label: &str, digest: &[u8]) -> HasherVector {
    HasherVector {
        name: name.to_string(),
        label: label.to_string(),
        absorb: "length_prefixed".to_string(),
        network: None,
        input_hex: to_hex(&input()),
        digest_hex: to_hex(digest),
    }
}

fn compute() -> Vectors {
    let domains = vec![
        domain_vector::<ConfidentialOutputHashDomain>("ConfidentialOutputHashDomain"),
        domain_vector::<TransactionSecureNonceKdfDomain>("TransactionSecureNonceKdfDomain"),
        domain_vector::<ValidatorNodeMerkleHashDomain>("ValidatorNodeMerkleHashDomain"),
        domain_vector::<WalletOutputEncryptionKeysDomain>("WalletOutputEncryptionKeysDomain"),
        domain_vector::<TransactionHashDomain>("TransactionHashDomain"),
        domain_vector::<LedgerHashDomain>("LedgerHashDomain"),
        domain_vector::<KeyManagerTransactionsHashDomain>("KeyManagerTransactionsHashDomain"),
        domain_vector::<PaymentReferenceHashDomain>("PaymentReferenceHashDomain"),
        domain_vector::<ValidatorNodeHashDomain>("ValidatorNodeHashDomain"),
        domain_vector::<KeyManagerDomain>("KeyManagerDomain"),
        domain_vector::<WalletOutputRewindKeysDomain>("WalletOutputRewindKeysDomain"),
        domain_vector::<WalletOutputSpendingKeysDomain>("WalletOutputSpendingKeysDomain"),
        domain_vector::<WalletMessageSigningDomain>("WalletMessageSigningDomain"),
        domain_vector_of_hasher("WalletHasher", &WalletHasher::new()),
        domain_vector::<BulletRangeProofHashDomain>("BulletRangeProofHashDomain"),
        domain_vector::<KernelMmrHashDomain>("KernelMmrHashDomain"),
        domain_vector::<InputMmrHashDomain>("InputMmrHashDomain"),
        domain_vector::<BlocksHashDomain>("BlocksHashDomain"),
        domain_vector::<OfflineSigningPayloadHashDomain>("OfflineSigningPayloadHashDomain"),
        domain_vector::<TariDanConsensusHashDomain>("TariDanConsensusHashDomain"),
    ];

    let input = input();
    let hashers = vec![
        borsh_vector(
            "layer2::validator_registration_hasher",
            "vn_registration",
            Some(NETWORK),
            &validator_registration_hasher(NETWORK).chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::validator_exit_hasher",
            "vn_exit",
            Some(NETWORK),
            &validator_exit_hasher(NETWORK).chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::block_hasher",
            "Block",
            None,
            &block_hasher().chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::block_metadata_hasher",
            "BlockMetadata",
            None,
            &block_metadata_hasher().chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::command_hasher",
            "Command",
            None,
            &command_hasher().chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::proposal_vote_signature_hasher",
            "VoteSignature",
            None,
            &proposal_vote_signature_hasher().chain(&input).finalize(),
        ),
        borsh_vector(
            "layer2::timeout_vote_signature_hasher",
            "TimeoutVoteSignature",
            None,
            &timeout_vote_signature_hasher().chain(&input).finalize(),
        ),
        borsh_vector(
            "DomainSeparatedBorshHasher<TransactionHashDomain, Blake2b<U32>>",
            "test_vector",
            None,
            &DomainSeparatedBorshHasher::<TransactionHashDomain, Blake2b<U32>>::new_with_label("test_vector")
                .chain(&input)
                .finalize(),
        ),
        length_prefixed_vector(
            "layer2::ValidatorNodeBmtHasherBlake2b",
            "",
            ValidatorNodeBmtHasherBlake2b::new().chain(input).finalize().as_ref(),
        ),
        length_prefixed_vector(
            "hashers::KernelMmrHasherBlake256",
            "",
            KernelMmrHasherBlake256::new().chain(input).finalize().as_ref(),
        ),
        length_prefixed_vector(
            "hashers::InputMmrHasherBlake256",
            "",
            InputMmrHasherBlake256::new().chain(input).finalize().as_ref(),
        ),
        length_prefixed_vector(
            "WalletHasher",
            "stealth_address",
            WalletHasher::new_with_label("stealth_address")
                .chain(input)
                .finalize()
                .as_ref(),
        ),
    ];

    Vectors { domains, hashers }
}

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("vectors.json")
}

#[test]
fn known_answer_vectors() {
    let computed = compute();
    let json = serde_json::to_string_pretty(&computed).unwrap() + "\n";
    // Regenerating always fails the run, so a stray variable (e.g. in CI) cannot silently rewrite the vectors and
    // pass.
    if std::env::var("TARI_HASHING_REGENERATE_VECTORS").as_deref() == Ok("1") {
        fs::write(vectors_path(), &json).unwrap();
        panic!(
            "hashing/tests/vectors.json was regenerated. Review the diff, then re-run without \
             TARI_HASHING_REGENERATE_VECTORS."
        );
    }
    let pinned: Vectors = serde_json::from_str(&fs::read_to_string(vectors_path()).unwrap()).unwrap();
    assert_eq!(
        pinned, computed,
        "hashing/tests/vectors.json does not match the code. If the change is intentional, regenerate (see the module \
         docs) and review the diff. Computed:\n{json}"
    );
}

#[test]
fn pinned_tags_match_their_domain_strings() {
    // Guards the JSON itself: each tag_hex must be the encoding of its own domain/version, so a consumer can rely on
    // either field.
    let pinned: Vectors = serde_json::from_str(&fs::read_to_string(vectors_path()).unwrap()).unwrap();
    for d in pinned.domains {
        assert_eq!(d.tag_hex, to_hex(&tag_bytes(&d.domain, d.version)), "{}", d.name);
    }
}
