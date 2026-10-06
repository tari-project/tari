// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The two seeds the simulated device is loaded with.
//!
//! # Why two
//!
//! One fixed seed cannot tell "the device derives correctly" apart from "the device returns a constant". A table
//! captured against a single seed passes just as happily against a device whose derivation has been replaced by a
//! lookup table, a stuck buffer, or a hard coded array. The second seed is what makes the vectors say something:
//! every value in the table has to *change*, and change to a second set of values that were themselves derived.
//!
//! # Why these two
//!
//! [`SPECULOS_DEFAULT_SEED_MNEMONIC`] is Speculos' own published default, so the vectors can be reproduced by
//! anybody from inputs that are in the Speculos source tree - no "trust our capture" step.
//! [`ALTERNATE_SEED_MNEMONIC`] is the BIP-39 all-zero-entropy 24 word mnemonic, equally published and unmistakably
//! a test value.
//!
//! # These are test seeds. They are public. Never put them on hardware.
//!
//! Both mnemonics are printed in public source repositories and in this file. Any funds sent to an address derived
//! from either of them can be swept by anyone, instantly and permanently, without any compromise of your machine -
//! the attacker does not need to steal anything, they already have the seed. Restoring either of these onto a real
//! Ledger device, or into any wallet that will ever hold real value, loses that value. They exist so that a
//! simulator produces the same keys on every machine, and for nothing else.

/// Speculos' default BIP-39 mnemonic, from `speculos/main.py::DEFAULT_SEED`.
///
/// **A published test seed. See the module docs. Never restore this onto real hardware.**
pub const SPECULOS_DEFAULT_SEED_MNEMONIC: &str = "glory promote mansion idle axis finger extra february uncover one \
                                                  trip resource lawn turtle enact monster seven myth punch hobby \
                                                  comfort wild raise skin";

/// The BIP-39 all-zero-entropy 24 word mnemonic, used as the second seed.
///
/// **A published test seed. See the module docs. Never restore this onto real hardware.**
pub const ALTERNATE_SEED_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                                           abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                                           abandon abandon abandon abandon abandon art";

/// The 64 byte seed [`SPECULOS_DEFAULT_SEED_MNEMONIC`] expands to.
///
/// Speculos does not run BIP-39 at start-up for its default; it carries the expanded bytes literally, as
/// `default_seed` in `speculos/src/environment.c`. Those are the bytes transcribed here, so that
/// `oracle::bip39_seed` can be checked against a value that came from the simulator rather than from itself.
pub const SPECULOS_DEFAULT_SEED_BYTES: &[u8; 64] = &[
    0xb1, 0x19, 0x97, 0xfa, 0xff, 0x42, 0x0a, 0x33, 0x1b, 0xb4, 0xa4, 0xff, 0xdc, 0x8b, 0xdc, 0x8b, 0xa7, 0xc0, 0x17,
    0x32, 0xa9, 0x9a, 0x30, 0xd8, 0x3d, 0xbb, 0xeb, 0xd4, 0x69, 0x66, 0x6c, 0x84, 0xb4, 0x7d, 0x09, 0xd3, 0xf5, 0xf4,
    0x72, 0xb3, 0xb9, 0x38, 0x4a, 0xc6, 0x34, 0xbe, 0xba, 0x2a, 0x44, 0x0b, 0xa3, 0x6e, 0xc7, 0x66, 0x11, 0x44, 0x13,
    0x2f, 0x35, 0xe2, 0x06, 0x87, 0x35, 0x64,
];

/// Which of the two seeds a running simulator was started with.
///
/// The vector table is indexed by this rather than by a raw mnemonic, because the thing a test needs to know is
/// "which column of expected values applies", and that is a closed set of two.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SeedId {
    /// Speculos' published default. What a simulator started with no `--seed` argument uses.
    SpeculosDefault,
    /// The second seed, passed to Speculos with `--seed`.
    Alternate,
}

impl SeedId {
    /// Both seeds, for tests that have to cover the pair.
    pub const ALL: [SeedId; 2] = [SeedId::SpeculosDefault, SeedId::Alternate];

    /// The mnemonic to hand Speculos' `--seed` argument.
    pub const fn mnemonic(self) -> &'static str {
        match self {
            SeedId::SpeculosDefault => SPECULOS_DEFAULT_SEED_MNEMONIC,
            SeedId::Alternate => ALTERNATE_SEED_MNEMONIC,
        }
    }

    /// The name used on the command line and in `SPECULOS_SEED_ID`.
    pub const fn name(self) -> &'static str {
        match self {
            SeedId::SpeculosDefault => "default",
            SeedId::Alternate => "alternate",
        }
    }

    /// Parse the name back, for reading `SPECULOS_SEED_ID`.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "default" => Some(SeedId::SpeculosDefault),
            "alternate" => Some(SeedId::Alternate),
            _ => None,
        }
    }
}

/// The 64 byte seed for `seed`, expanded through BIP-39.
pub fn seed_bytes(seed: SeedId) -> zeroize::Zeroizing<[u8; 64]> {
    crate::oracle::bip39_seed(seed.mnemonic(), "")
}

#[cfg(test)]
mod test {
    use tari_utilities::hex::{Hex, from_hex};

    use super::*;

    /// A mnemonic with a stray double space, or a word dropped when the constant was re-wrapped across lines, would
    /// silently derive a different seed and invalidate every vector. Word count and the `\` line continuations are
    /// the two things most likely to go wrong, so both are asserted.
    #[test]
    fn the_mnemonics_are_well_formed() {
        for seed in SeedId::ALL {
            let words: Vec<&str> = seed.mnemonic().split(' ').collect();
            assert_eq!(words.len(), 24, "{} is not 24 words", seed.name());
            assert!(
                words.iter().all(|w| !w.is_empty()),
                "{} has a doubled space in it",
                seed.name()
            );
        }
    }

    /// The two seeds must actually be different, which is the entire premise of having two.
    #[test]
    fn the_two_seeds_differ() {
        assert_ne!(
            seed_bytes(SeedId::SpeculosDefault).as_ref(),
            seed_bytes(SeedId::Alternate).as_ref()
        );
    }

    /// `SeedId` round trips through the name used on the command line and in the environment variable.
    #[test]
    fn seed_names_round_trip() {
        for seed in SeedId::ALL {
            assert_eq!(SeedId::from_name(seed.name()), Some(seed));
        }
        assert_eq!(SeedId::from_name("mainnet-please"), None);
    }

    /// The transcribed Speculos bytes are readable as hex, which is how they appear in the docs and scripts.
    #[test]
    fn the_transcribed_seed_matches_its_hex() {
        let as_hex = SPECULOS_DEFAULT_SEED_BYTES.to_vec().to_hex();
        assert_eq!(from_hex(&as_hex).unwrap(), SPECULOS_DEFAULT_SEED_BYTES.to_vec());
        assert_eq!(as_hex.len(), 128);
    }
}
