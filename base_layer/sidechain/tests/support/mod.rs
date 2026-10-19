// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::de::DeserializeOwned;
use tari_common_types::types::CompressedPublicKey;
use tari_sidechain::SigningCommittee;
use tari_utilities::{ByteArray, hex::Hex};

pub fn load_fixture<T: DeserializeOwned>(name: &str) -> T {
    let path = format!("tests/fixtures/{name}");
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}

/// The committee that signed the certificates in `commit_proof.json`, each member with power 1.
pub fn fixture_committee() -> TestCommittee {
    TestCommittee::new(
        [
            "6efb3b5ede2c8abba5280b58cd5e3bd1887101f7ffb973cf46cb2542bdbd202f",
            "9e504f9b10c40230ec4e1570dcf295d5da01aa0daeced57316b160c80bc57e0c",
            "a45af5c5eeb3db9687fe9edae95387f91ff5a90c442159204c3f97514dddad7c",
            "cec1426a33965eb2a7d82b281964ad39f06d6fba7d8e57f8da4fcfefd946d855",
        ]
        .into_iter()
        .map(|hex| (CompressedPublicKey::from_hex(hex).unwrap(), 1)),
    )
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
