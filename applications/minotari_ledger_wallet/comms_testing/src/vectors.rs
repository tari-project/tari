// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The shared key derivation vector table.
//!
//! # One table, every model
//!
//! There is exactly one table here and it is asserted against **every** device model the harness runs. That is not
//! a convenience; it is the assertion. "The crypto is target independent" is otherwise an assumption nobody ever
//! checks, and the `stax`/`flex` half of the device application is a genuinely different build - 79 `target_os`
//! sites across 11 files, including a different main event loop.
//!
//! **If `stax` disagrees with `nanosplus` about a derived key, that is a bug in the device application.** It is not
//! fixed by giving `stax` its own table. A user who moves their recovery phrase from one Ledger model to another
//! and finds a different wallet has lost their funds; a forked table would hide exactly that. The correct response
//! to a disagreement is to fix the device, or - if the device is right and the table is wrong - to change the one
//! shared table and understand why it moved.
//!
//! # What is in here, and what is deliberately not
//!
//! Key derivation only. There are no signature vectors, because a frozen signature is a weaker statement than the
//! one the scenario suite already makes: a signature can be verified against its own public key, cryptographically,
//! without any stored expectation at all. Freezing one would only pin the nonce, which is random by design.
//!
//! # Where these numbers came from
//!
//! Captured from Speculos running the `nanosplus` and `stax` builds of the device application, and independently
//! reproduced by [`crate::oracle`] from BIP-39, BIP-32 and the published Tari domain separated hashing. Both models
//! returned byte identical values for every row and both seeds. `test::the_oracle_reproduces_every_vector` fails
//! the build if the table and the oracle ever stop agreeing.

use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use tari_utilities::ByteArray;

use crate::{
    oracle::{self, KeyType, OracleError},
    seeds::{SeedId, seed_bytes},
};

/// The device instruction a vector exercises.
///
/// Each of these bottoms out in the same `derive_from_bip32_key(account, index, key_type)`; they differ in which
/// `index`/`key_type` the device picks and whether it returns the public or the secret half.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeviceCall {
    /// `GetPublicKey`: host supplied index and branch, returns the **public** key.
    PublicKey { index: u64, branch: LedgerKeyBranch },
    /// `GetPublicSpendKey`: fixed index 42, `KeyType::Spend`, returns the **public** key. The branch is not
    /// reachable from the host - `KeyType::from_branch_key` rejects `LedgerKeyBranch::Spend` - so this instruction
    /// is the only way to observe it.
    PublicSpendKey,
    /// `GetViewKey`: fixed index 57311, `KeyType::ViewKey`, returns the **secret** key. A view key is meant to be
    /// shareable, which is why the device parts with the scalar itself here and nowhere else.
    ViewKey,
}

/// One row: a device call, and the 32 bytes the device must answer with under each seed.
#[derive(Debug, Copy, Clone)]
pub struct DerivationVector {
    /// A name that appears in assertion failures, so a broken row is identifiable without counting indexes.
    pub name: &'static str,
    pub account: u64,
    pub call: DeviceCall,
    /// Expected bytes under [`SeedId::SpeculosDefault`], hex encoded.
    pub speculos_default: &'static str,
    /// Expected bytes under [`SeedId::Alternate`], hex encoded.
    pub alternate: &'static str,
}

impl DerivationVector {
    /// The expected value for `seed`.
    pub const fn expected(&self, seed: SeedId) -> &'static str {
        match seed {
            SeedId::SpeculosDefault => self.speculos_default,
            SeedId::Alternate => self.alternate,
        }
    }

    /// Recompute this row from first principles, without a device.
    ///
    /// Returns the 32 bytes the device should answer with - the compressed Ristretto public key, or for
    /// [`DeviceCall::ViewKey`] the secret scalar.
    pub fn oracle(&self, seed: SeedId) -> Result<[u8; 32], OracleError> {
        let seed = seed_bytes(seed);
        let bytes = match self.call {
            DeviceCall::PublicKey { index, branch } => {
                oracle::derive_public_key(seed.as_ref(), self.account, index, key_type_for(branch))?
                    .as_bytes()
                    .to_vec()
            },
            DeviceCall::PublicSpendKey => oracle::derive_public_spend_key(seed.as_ref(), self.account)?
                .as_bytes()
                .to_vec(),
            DeviceCall::ViewKey => oracle::derive_view_key(seed.as_ref(), self.account)?
                .as_bytes()
                .to_vec(),
        };
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes.get(..32).unwrap_or_default());
        Ok(out)
    }
}

/// The host's branch identifier mapped onto the device's key type, mirroring `KeyType::from_branch_key` in
/// `wallet/src/main.rs`.
///
/// The two numbering schemes are different and neither is a subset of the other, so this is a real translation and
/// not a cast. `LedgerKeyBranch::Spend` has no mapping at all - the device answers `BadBranchKey` - which is why it
/// is absent from every vector and why [`DeviceCall::PublicSpendKey`] exists as its own call.
pub const fn key_type_for(branch: LedgerKeyBranch) -> KeyType {
    match branch {
        LedgerKeyBranch::OneSidedSenderOffset => KeyType::OneSidedSenderOffset,
        LedgerKeyBranch::Random => KeyType::Random,
        LedgerKeyBranch::PreMine => KeyType::PreMine,
        // Unreachable from a vector: no handler will derive this on the host's say-so. Mapping it to the key type
        // the device uses internally keeps this function total without inventing a branch the device would accept.
        LedgerKeyBranch::Spend => KeyType::Spend,
    }
}

/// How many rows [`DERIVATION_VECTORS`] must have.
///
/// Every assertion over the table is a `for` loop, and a `for` loop over an empty slice passes. Without this
/// constant, a botched conflict resolution or a stray delete that truncated the table would leave every test in
/// this crate - and every device test - reporting green having checked nothing at all. That is precisely the
/// silence this table exists to prevent, so the row count is pinned and changing it has to be deliberate.
///
/// Raise it when you add a vector. If you are *lowering* it, be sure you know which property stopped being worth
/// asserting.
pub const EXPECTED_VECTOR_COUNT: usize = 11;

/// The shared vector table.
pub const DERIVATION_VECTORS: &[DerivationVector] = &[
    DerivationVector {
        name: "public_key/account 0/index 0/one-sided sender offset",
        account: 0,
        call: DeviceCall::PublicKey {
            index: 0,
            branch: LedgerKeyBranch::OneSidedSenderOffset,
        },
        speculos_default: "aa72f8fd4a339566c62a9cd525ac5f08579a2a30f43fac15aef4aaa5541c1f59",
        alternate: "ec5b0c357b858714b59e0b3a3255c82dd17c624fb469bad51ac465145a90ca7a",
    },
    DerivationVector {
        name: "public_key/account 0/index 1/random",
        account: 0,
        call: DeviceCall::PublicKey {
            index: 1,
            branch: LedgerKeyBranch::Random,
        },
        speculos_default: "d02ff474b1303d6cb6ec1432e4e9d64aec74109caf798129a1dc89bfdc295a13",
        alternate: "683244b611829eee1982d6779f0e536be7bb68a12be0af859f8417f542eb6260",
    },
    DerivationVector {
        name: "public_key/account 0/index 7/pre-mine",
        account: 0,
        call: DeviceCall::PublicKey {
            index: 7,
            branch: LedgerKeyBranch::PreMine,
        },
        speculos_default: "7ccf5a295d39d19909ea240177a4f6072b8f3cefe5d9f8d5da0e25ee69cbc822",
        alternate: "c084c756194948a4b184aa9d3a192eb48c1181078d931e24d5d6f1cda807d146",
    },
    // The account is a path element, so changing it alone must change the key. Paired with the row above it, this
    // is what stops an implementation that ignores its account argument from passing.
    DerivationVector {
        name: "public_key/account 1/index 0/one-sided sender offset",
        account: 1,
        call: DeviceCall::PublicKey {
            index: 0,
            branch: LedgerKeyBranch::OneSidedSenderOffset,
        },
        speculos_default: "d8b988abf67175d2373062c1c594d158961145e2c547ee11be21ea9538da632b",
        alternate: "1e7f9e9ffc8ad06452389703bf05110d74c9a95f7dc620aba1c7f1ebf02f284c",
    },
    DerivationVector {
        name: "public_key/account 42/index 12345/random",
        account: 42,
        call: DeviceCall::PublicKey {
            index: 12345,
            branch: LedgerKeyBranch::Random,
        },
        speculos_default: "768f68bc3abf5dca07d14629984af27181b9aa25afb9c734b3c03418d137ff66",
        alternate: "c0bc1b92e776a1e2292142898f9751601378d11ab3bdd39aaf602849348e717b",
    },
    // The largest account and index that survive `make_bip32_path`'s `u32` accumulator without wrapping. These pin
    // the top of the usable range; the wrap past it is covered by `ACCOUNT_WRAP_VECTOR`.
    DerivationVector {
        name: "public_key/account u32::MAX/index 0/random",
        account: u32::MAX as u64,
        call: DeviceCall::PublicKey {
            index: 0,
            branch: LedgerKeyBranch::Random,
        },
        speculos_default: "d23cf77443e495d46fe01b7076ebdce5079969721806a0b8143a81403e7a3f5d",
        alternate: "de9bec40433b26db8944626f27fbfd4776895c63de42b3f141fc7c5084c76e6c",
    },
    DerivationVector {
        name: "public_key/account 0/index u32::MAX/pre-mine",
        account: 0,
        call: DeviceCall::PublicKey {
            index: u32::MAX as u64,
            branch: LedgerKeyBranch::PreMine,
        },
        speculos_default: "deab8e8114f40578edc82f1d49c837b98c57d6ec1c3dc096a56ea5b9d9cfd626",
        alternate: "d2bf89c3ef39a884d0d9b79bd2d594f52e69c124aa88303089e83e0ceefa7f2c",
    },
    DerivationVector {
        name: "public_spend_key/account 0",
        account: 0,
        call: DeviceCall::PublicSpendKey,
        speculos_default: "f40a25bbf628ead50dc3667e0052b9688bd3c7f13a7867d795db51744d024c58",
        alternate: "a4dd7ee6c71ae686cd13b7c2cf1ddd7a1fae0e0a3c24fa86b95d73dc32c7d838",
    },
    DerivationVector {
        name: "public_spend_key/account 1",
        account: 1,
        call: DeviceCall::PublicSpendKey,
        speculos_default: "8c15a5a577e3dd2576f07290fc8c006bf6eaa0d91b656faec53bd15d90ab3e7d",
        alternate: "c02b96a5921ae4dccc43c9f04dae5f35eb999a5d357762e85df7a0a726709c2d",
    },
    // The one instruction that returns a secret scalar rather than a point, so it is the only row where a
    // disagreement would be visible *before* the curve multiplication.
    DerivationVector {
        name: "view_key/account 0",
        account: 0,
        call: DeviceCall::ViewKey,
        speculos_default: "2865e78817eb4e53b0ad8a3e700c61b5bc9d5cd5905b7fca32b4539a31b8f101",
        alternate: "89f9e38c79d339c78c5a1b6e7debfeb6e3e6db634acd7c1a715ea9c36a26a20b",
    },
    DerivationVector {
        name: "view_key/account 1",
        account: 1,
        call: DeviceCall::ViewKey,
        speculos_default: "3f9c914014067bfa26b3220d839c5819c315b07e2c8af27a0d2b1cc9f3b0950c",
        alternate: "3c115bb07b1741a018e0e224eae91249d5f9c13f759b953233fbdc7c52f7d005",
    },
];

/// An account that overflows `make_bip32_path`'s `u32` accumulator, and the account it therefore collides with.
///
/// This is kept out of [`DERIVATION_VECTORS`] because it is not a key anybody should ever ask for - it is a
/// *property*, and the property is a sharp edge rather than a feature: the host sends `u64` accounts (see
/// `accessor_methods.rs`, which fills them from `rand::rng().next_u64()`), the device renders one into a decimal
/// string, and `make_bip32_path` parses it back with `acc * 10 + digit` into a `u32`. The device application is a
/// release build, so that wraps instead of panicking, and accounts 2^32 apart address the same key.
///
/// The table below is what a device returns today, confirmed on `nanosplus` and `stax`. Freezing it means that if
/// the SDK ever starts rejecting or saturating instead of wrapping, the change is caught here rather than found by
/// a user whose account silently moved.
pub const ACCOUNT_WRAP_VECTOR: (u64, u64, LedgerKeyBranch) = (0, 1u64 << 32, LedgerKeyBranch::Random);

#[cfg(test)]
mod test {
    use tari_utilities::hex::Hex;

    use super::*;

    /// The table has the rows it is supposed to have.
    ///
    /// This is the guard that makes every other assertion in this crate mean something. All of them iterate the
    /// table, and iterating an empty table succeeds, so a truncated table would turn the whole suite - including
    /// the device tests - into a green run that checked nothing.
    #[test]
    fn the_table_has_the_expected_number_of_rows() {
        assert!(!DERIVATION_VECTORS.is_empty(), "the vector table is empty");
        assert_eq!(
            DERIVATION_VECTORS.len(),
            EXPECTED_VECTOR_COUNT,
            "the vector table has {} rows but EXPECTED_VECTOR_COUNT says {}. If you added or removed a vector on \
             purpose, update the constant; if you did not, something has eaten part of the table.",
            DERIVATION_VECTORS.len(),
            EXPECTED_VECTOR_COUNT
        );
    }

    /// Every device call the table can express is actually exercised by at least one row.
    ///
    /// A row count alone would still pass if all eleven rows were `GetPublicKey`. Each instruction reaches the same
    /// derivation by a different route - two of them pick the index and key type on the device rather than taking
    /// them from the host - so losing all the rows for one of them loses that coverage silently.
    #[test]
    fn every_device_call_is_covered() {
        let has = |f: fn(&DeviceCall) -> bool| DERIVATION_VECTORS.iter().any(|v| f(&v.call));
        assert!(
            has(|c| matches!(c, DeviceCall::PublicKey { .. })),
            "no GetPublicKey vectors"
        );
        assert!(
            has(|c| matches!(c, DeviceCall::PublicSpendKey)),
            "no GetPublicSpendKey vectors"
        );
        assert!(has(|c| matches!(c, DeviceCall::ViewKey)), "no GetViewKey vectors");
    }

    /// The oracle must reproduce every published value, for both seeds. This is what turns the table from frozen
    /// into verified: the numbers are checkable against BIP-39, BIP-32 and the Tari hashing construction, by
    /// anybody, without a device.
    #[test]
    fn the_oracle_reproduces_every_vector() {
        assert_eq!(DERIVATION_VECTORS.len(), EXPECTED_VECTOR_COUNT);
        for vector in DERIVATION_VECTORS {
            for seed in SeedId::ALL {
                let derived = vector.oracle(seed).expect("the oracle must derive every vector");
                assert_eq!(
                    derived.to_vec().to_hex(),
                    vector.expected(seed),
                    "oracle disagrees with the table for '{}' under the {} seed",
                    vector.name,
                    seed.name()
                );
            }
        }
    }

    /// Swapping the seed must change **every** vector. A row that did not move would mean the device (and the
    /// oracle) could be answering that row from something other than the seed.
    #[test]
    fn the_second_seed_changes_every_vector() {
        assert_eq!(DERIVATION_VECTORS.len(), EXPECTED_VECTOR_COUNT);
        for vector in DERIVATION_VECTORS {
            assert_ne!(
                vector.speculos_default, vector.alternate,
                "'{}' is identical under both seeds, so it proves nothing",
                vector.name
            );
        }
    }

    /// Distinct rows must have distinct values. Two rows that collide would be a table that looks like it covers
    /// more ground than it does - and, more importantly, would be the signature of a device ignoring one of its
    /// arguments.
    #[test]
    fn every_vector_is_distinct() {
        assert_eq!(DERIVATION_VECTORS.len(), EXPECTED_VECTOR_COUNT);
        for seed in SeedId::ALL {
            let mut seen: Vec<(&str, &str)> = Vec::new();
            for vector in DERIVATION_VECTORS {
                if let Some((other, _)) = seen.iter().find(|(_, v)| *v == vector.expected(seed)) {
                    panic!(
                        "'{}' and '{}' derive the same key under the {} seed",
                        other,
                        vector.name,
                        seed.name()
                    );
                }
                seen.push((vector.name, vector.expected(seed)));
            }
        }
    }

    /// Names are what an assertion failure prints, so a duplicate name would point at the wrong row.
    #[test]
    fn every_vector_name_is_unique() {
        assert_eq!(DERIVATION_VECTORS.len(), EXPECTED_VECTOR_COUNT);
        let mut names: Vec<&str> = DERIVATION_VECTORS.iter().map(|v| v.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two vectors share a name");
    }

    /// Every expected value must be 32 bytes of hex. A truncated paste would otherwise fail much later, inside a
    /// key parse, with a far less obvious message.
    #[test]
    fn every_expected_value_is_32_bytes_of_hex() {
        assert_eq!(DERIVATION_VECTORS.len(), EXPECTED_VECTOR_COUNT);
        for vector in DERIVATION_VECTORS {
            for seed in SeedId::ALL {
                let expected = vector.expected(seed);
                assert_eq!(
                    expected.len(),
                    64,
                    "'{}' under {} is not 32 bytes",
                    vector.name,
                    seed.name()
                );
                assert!(
                    expected
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "'{}' under {} is not lower case hex",
                    vector.name,
                    seed.name()
                );
            }
        }
    }

    /// The wrapping account and the account it collides with must derive the same key according to the oracle,
    /// which is the host side half of `the_account_index_wraps_at_u32` in the device suite.
    #[test]
    fn the_oracle_agrees_that_the_account_wraps_at_u32() {
        let (low, wrapping, branch) = ACCOUNT_WRAP_VECTOR;
        for seed in SeedId::ALL {
            let bytes = seed_bytes(seed);
            let a = oracle::derive_public_key(bytes.as_ref(), low, 0, key_type_for(branch)).unwrap();
            let b = oracle::derive_public_key(bytes.as_ref(), wrapping, 0, key_type_for(branch)).unwrap();
            assert_eq!(a, b, "the account did not wrap at 2^32 under the {} seed", seed.name());
        }
    }

    /// The branch to key type translation is a real remapping, not an identity. If someone "simplifies" it into a
    /// cast, every sender offset and pre-mine vector silently changes.
    #[test]
    fn the_branch_mapping_is_not_the_identity() {
        assert_eq!(key_type_for(LedgerKeyBranch::OneSidedSenderOffset).as_byte(), 0x04);
        assert_ne!(LedgerKeyBranch::OneSidedSenderOffset.as_byte(), 0x04);
        assert_eq!(key_type_for(LedgerKeyBranch::Random).as_byte(), 0x06);
        assert_eq!(key_type_for(LedgerKeyBranch::PreMine).as_byte(), 0x07);
    }
}
