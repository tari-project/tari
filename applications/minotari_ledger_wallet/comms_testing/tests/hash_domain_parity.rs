// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Host-side parity check between the Ledger app's hash domains and `tari_hashing`.
//!
//! The device application cannot link `tari_hashing` (its `tari_crypto` dependency is not `no_std`), so it carries
//! its own copy of the three domains it uses. This test pins that copy - and the redeclarations in this crate's
//! `fixtures` and `oracle` - against `hashing/tests/vectors.json`, the known-answer file `tari_hashing` itself is
//! tested against. It needs no simulator and is deliberately neither `#[ignore]`d nor cfg gated.

use std::{fs, path::PathBuf};

use minotari_ledger_wallet_comms_testing::{fixtures, oracle};
use serde_json::Value;
use tari_crypto::hashing::DomainSeparation;

/// (name, domain, version) of every domain the device application declares.
const DEVICE_DOMAINS: [(&str, &str, u8); 3] = [
    ("LedgerHashDomain", "com.tari.minotari_ledger_wallet", 0),
    (
        "KeyManagerTransactionsHashDomain",
        "com.tari.base_layer.core.transactions.key_manager",
        1,
    ),
    ("TransactionHashDomain", "com.tari.base_layer.core.transactions", 0),
];

fn repo_root() -> PathBuf {
    // applications/minotari_ledger_wallet/comms_testing -> repository root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn vectors() -> Value {
    let path = repo_root().join("hashing/tests/vectors.json");
    serde_json::from_str(&fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())))
        .expect("vectors.json is valid JSON")
}

fn tag_hex(domain: &str, version: u8) -> String {
    let tag = format!("{domain}.v{version}");
    let mut bytes = (tag.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(tag.as_bytes());
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn device_domains_match_the_tari_hashing_vectors() {
    let vectors = vectors();
    let domains = vectors["domains"].as_array().expect("domains array");
    for (name, domain, version) in DEVICE_DOMAINS {
        let entry = domains
            .iter()
            .find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from hashing/tests/vectors.json"));
        assert_eq!(entry["domain"], domain, "{name}");
        assert_eq!(entry["version"], u64::from(version), "{name}");
        assert_eq!(entry["tag_hex"], tag_hex(domain, version), "{name}");
    }
}

/// Domains the device declares that are not `tari_hashing` domains: they are copies of `tari_crypto`'s Schnorr
/// challenge domain and `tari_script`'s CheckSig domain, and are exercised by the signature scenarios instead.
const DEVICE_DOMAINS_OUTSIDE_TARI_HASHING: [&str; 2] = ["SchnorrSigChallenge", "CheckSigHashDomain"];

fn read_rs_files(dir: &PathBuf, out: &mut String) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push_str(&fs::read_to_string(&path).unwrap());
        }
    }
}

#[test]
fn device_source_declares_exactly_these_domains() {
    // The device crate cannot be linked from the host, so read its declarations as text.
    let mut source = String::new();
    read_rs_files(
        &repo_root().join("applications/minotari_ledger_wallet/wallet/src"),
        &mut source,
    );
    let source = strip_whitespace(&source);
    for (name, domain, version) in DEVICE_DOMAINS {
        let decl = format!("hash_domain!({name},\"{domain}\",{version});");
        assert!(source.contains(&decl), "device source does not declare {decl}");
    }

    // Every domain the device declares must be accounted for, so that a new one cannot skip this check.
    for decl in source.split("hash_domain!(").skip(1) {
        let name = decl.split(',').next().unwrap_or_default();
        if name.starts_with('$') {
            // The macro definition itself
            continue;
        }
        assert!(
            DEVICE_DOMAINS.iter().any(|(n, _, _)| *n == name) || DEVICE_DOMAINS_OUTSIDE_TARI_HASHING.contains(&name),
            "the device declares hash domain {name}, which this test does not know about; if it is a tari_hashing \
             domain add it to DEVICE_DOMAINS"
        );
    }
}

fn assert_domain<M: DomainSeparation>(name: &str) {
    let (_, domain, version) = DEVICE_DOMAINS
        .iter()
        .find(|(n, _, _)| *n == name)
        .unwrap_or_else(|| panic!("{name} is not a device domain"));
    assert_eq!(M::domain(), *domain, "{name}");
    assert_eq!(M::version(), *version, "{name}");
}

#[test]
fn comms_testing_redeclarations_match() {
    assert_domain::<oracle::LedgerHashDomain>("LedgerHashDomain");
    assert_domain::<fixtures::KeyManagerTransactionsHashDomain>("KeyManagerTransactionsHashDomain");
    assert_domain::<fixtures::TransactionHashDomain>("TransactionHashDomain");
}
