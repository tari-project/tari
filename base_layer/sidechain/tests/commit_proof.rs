//   Copyright 2025 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use tari_sidechain::SidechainBlockCommitProof;

mod support;
use support::load_fixture;

#[test]
fn it_validates_a_correct_proof() {
    let proof = load_fixture::<SidechainBlockCommitProof>("commit_proof.json");
    proof.validate_committed(4, &|_| Ok(true)).unwrap();
}
