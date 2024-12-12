// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_sidechain::EvictionProof;

mod support;

mod validate {
    use super::*;
    #[test]
    fn it_validates_a_valid_proof() {
        let proof = support::load_fixture::<EvictionProof>("eviction_proof1.json");
        proof.validate(4, &|_| Ok(true)).unwrap();
    }

    #[test]
    fn it_rejects_if_qc_signs_for_unknown_validator() {
        let proof = support::load_fixture::<EvictionProof>("eviction_proof1.json");
        proof.validate(4, &|_| Ok(false)).unwrap_err();
    }
}
