// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The key derivation vector table, asserted against whatever device is running.
//!
//! This is where Spec 2's table ([`crate::vectors`]) meets a device. The table's *internal* properties - that the
//! oracle reproduces every row, that the two seeds disagree everywhere, that no two rows collide - are unit tests
//! in [`crate::vectors`] and need no device at all. What is here is the one statement only a device can make:
//! **this device derives those keys.**
//!
//! # One table, every model
//!
//! The same table is asserted against `nanosplus` and against `stax`, unmodified. If they disagree about a derived
//! key that is a bug in the device application - a user who moves their recovery phrase between Ledger models would
//! find a different wallet - and it is fixed in the device, never by forking the table.
//!
//! # The seed is identified by asking the device, not by being told
//!
//! [`identify_seed`] probes one public-key row and works out which of the two published seeds the device holds.
//! That is what lets one scenario library run unchanged on a simulator, where the harness started the simulator and
//! knows the seed, and on hardware, where it does not. `SPECULOS_SEED_ID` is still cross checked when it is set,
//! because a harness that started the *wrong* simulator is a failure worth naming on its own.
//!
//! # Nothing here transcribes an answer from a device it cannot identify
//!
//! Every assertion below prints the device's raw answer on failure - `assert_eq!` and `assert_ne!` both dump their
//! operands, and so does every message this module builds - and `scripts/ledger_speculos.sh` copies the resulting
//! JUnit XML into an artifact directory that CI uploads. One of the instructions in the table, `GetViewKey`,
//! returns a **secret** scalar.
//!
//! Under the two published seeds that is harmless: both mnemonics are in this repository. The problem is that
//! nothing constrains the harness to those seeds. `SPECULOS_APDU_ADDRESS` is free form and will happily point at
//! anything speaking APDU over TCP, and the hardware frontend talks to whatever Ledger is plugged in. Aimed at a
//! device holding a real recovery phrase, a single mismatching row would write live key material into a file built
//! to be uploaded.
//!
//! So [`identify_seed`] is a precondition, not redaction. It asks one canonical *public key* question whose answer
//! is already in the table, and every scenario here refuses to go on unless the answer is one of the two published
//! seeds' values. A device that fails it has its answers withheld entirely - the failure message deliberately
//! contains no device output.

use std::sync::OnceLock;

use minotari_ledger_wallet_common::common_types::Instruction;
use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_get_public_key,
    ledger_get_public_spend_key,
    ledger_get_view_key,
};
use tari_utilities::{ByteArray, hex::Hex};

use crate::{
    scenarios::{
        Approval,
        Scenario,
        ScenarioContext,
        ScenarioError,
        ScenarioModule,
        ScenarioResult,
        WithContext,
        fail,
        require,
    },
    seeds::SeedId,
    simulator,
    vectors::{ACCOUNT_WRAP_VECTOR, DERIVATION_VECTORS, DerivationVector, DeviceCall, EXPECTED_VECTOR_COUNT},
};

pub const MODULE: ScenarioModule = ScenarioModule {
    name: "vectors",
    scenarios: SCENARIOS,
};

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "every derivation vector matches the device, and no row matches the other seed's value",
        covers: &[
            Instruction::GetPublicKey,
            Instruction::GetPublicSpendKey,
            Instruction::GetViewKey,
        ],
        approval: Approval::NotNeeded,
        run: the_table_matches_the_device,
    },
    Scenario {
        name: "derivation is a pure function of its inputs, across calls",
        covers: &[
            Instruction::GetPublicKey,
            Instruction::GetPublicSpendKey,
            Instruction::GetViewKey,
        ],
        approval: Approval::NotNeeded,
        run: derivation_is_deterministic,
    },
    Scenario {
        name: "the account path element wraps at 2^32, as make_bip32_path's u32 accumulator does",
        covers: &[Instruction::GetPublicKey],
        approval: Approval::NotNeeded,
        run: the_account_wraps_at_u32,
    },
];

/// Fail if the table is not the size it is supposed to be.
///
/// Every scenario below is a `for` loop over `DERIVATION_VECTORS`, and a `for` loop over an empty slice succeeds.
/// Without this, a truncated table would let the whole device suite report green against a live device having
/// asserted nothing whatsoever - the exact silence the table exists to prevent, arriving in the one place nobody
/// would think to look for it.
fn require_table_is_populated() -> ScenarioResult {
    require(DERIVATION_VECTORS.len() == EXPECTED_VECTOR_COUNT, || {
        format!(
            "the vector table has {} rows, expected {EXPECTED_VECTOR_COUNT}; the device assertions would be vacuous",
            DERIVATION_VECTORS.len()
        )
    })
}

/// Which of the two published test seeds this device holds.
///
/// See the module docs: this doubles as the "is this a device whose answers may be written down" precondition, and
/// the verdict it produces on failure carries no device output at all.
///
/// # Only a success is cached
///
/// The probe costs an exchange, so a *successful* identification is remembered for the rest of the process; a
/// device does not change its seed mid-run.
///
/// A failure is not cached, and that distinction matters more than it looks. `ask_device` returns an error for any
/// reason at all - a transport timeout, a dropped connection, the application not being open - and an earlier
/// version of this function turned every one of those into the "did not answer with either published test seed's
/// value" verdict, then remembered it. That verdict is the most alarming message this crate can produce: it tells
/// an operator their device may be holding a real recovery phrase and that its output was withheld for safety.
/// Producing it because a socket blipped is wrong on its own, and caching it means one blip poisons every vector
/// scenario for the rest of the run with a message pointing at entirely the wrong problem.
///
/// So a failure to *ask* is propagated with its own context and the next caller tries again. Only "the device
/// answered, and the answer matched neither seed" reaches the security verdict.
pub fn identify_seed() -> Result<SeedId, ScenarioError> {
    static SEED: OnceLock<SeedId> = OnceLock::new();

    if let Some(seed) = SEED.get() {
        return Ok(*seed);
    }

    // A public-key row on purpose: the probe itself must not be the thing that extracts a secret.
    let canonical = DERIVATION_VECTORS
        .iter()
        .find(|vector| matches!(vector.call, DeviceCall::PublicKey { .. }))
        .ok_or_else(|| fail("the vector table has no GetPublicKey row to identify the device with"))?;
    // Not `.ok()?`. An error here is a failure to *ask*, and it must not be reported as an answer.
    let actual = ask_device(canonical).context(|| {
        format!(
            "could not ask the device for '{}' to work out which seed it holds",
            canonical.name
        )
    })?;
    let identified = SeedId::ALL.into_iter().find(|seed| canonical.expected(*seed) == actual);

    let Some(seed) = identified else {
        return Err(fail(format!(
            "The device at {} did not answer a canonical derivation with either published test seed's value, so this \
             suite will not print or record anything it returns.\n\nThese scenarios transcribe device answers - \
             including the GetViewKey secret scalar - into failure messages and into JUnit XML that CI uploads, so \
             they refuse to run against a device they cannot identify as a test device. If that is a real device or a \
             simulator holding a real recovery phrase, nothing was printed and nothing was written.\n\nFor the \
             simulator, point SPECULOS_APDU_ADDRESS at one started by ./scripts/ledger_speculos.sh. For hardware, \
             restore one of the two published test mnemonics in `seeds.rs` onto a device that will never hold value.",
            simulator::apdu_address().unwrap_or_else(|_| "<unset>".to_string())
        )));
    };
    // `set` rather than `get_or_init`, because the work above can fail and a `OnceLock` initialiser cannot. A
    // racing caller may already have won the race; either way the value is the same seed.
    let _first = SEED.set(seed);

    // Cross checked rather than trusted, and only when it is set. A harness that started the alternate-seed
    // simulator and then told the suite it was the default one would otherwise sail through every scenario here,
    // because the probe would simply pick the seed that matched - and the model/seed matrix would report that both
    // seeds were covered when one of them was run twice.
    if let Ok(Some(declared)) = simulator::seed_id_if_set() {
        require(declared == seed, || {
            format!(
                "SPECULOS_SEED_ID says the device holds the {} seed, but it answers with the {} seed's values. Either \
                 the harness started the wrong simulator, or the variable is wrong; in both cases the model/seed \
                 matrix is not covering what it says it is.",
                declared.name(),
                seed.name()
            )
        })?;
    }

    Ok(seed)
}

/// Ask the device for whatever `vector` describes, and return the 32 bytes it answered with, hex encoded.
///
/// Hex encoded rather than raw, and that is load bearing beyond readability: every caller interpolates this into a
/// failure message that reaches a terminal and the JUnit XML CI uploads. `to_hex` emits `[0-9a-f]` and nothing
/// else, so a device cannot put terminal escapes through it. Anything added here that returns *unencoded* device
/// bytes has to be written with `{:?}` at its print sites, the way `approver::Transcript` handles screen text.
fn ask_device(vector: &DerivationVector) -> Result<String, ScenarioError> {
    let bytes = match vector.call {
        DeviceCall::PublicKey { index, branch } => ledger_get_public_key(vector.account, index, branch)
            .context(|| format!("GetPublicKey for '{}'", vector.name))?
            .as_bytes()
            .to_vec(),
        DeviceCall::PublicSpendKey => ledger_get_public_spend_key(vector.account)
            .context(|| format!("GetPublicSpendKey for '{}'", vector.name))?
            .as_bytes()
            .to_vec(),
        DeviceCall::ViewKey => ledger_get_view_key(vector.account)
            .context(|| format!("GetViewKey for '{}'", vector.name))?
            .as_bytes()
            .to_vec(),
    };
    Ok(bytes.to_hex())
}

/// Acceptance: the shared vector table passes against whatever model and seed is running, unmodified.
///
/// Both halves are here rather than in two scenarios, because they are one statement about one set of answers and
/// splitting them would double the exchanges for nothing:
///
/// * every row matches the seed the device actually holds;
/// * **no** row matches the other seed's value. The table already asserts its two columns differ, which is a statement
///   about the table. This is the statement about the device: whichever seed it holds, it must not be answering with
///   the other one's values. Run under both seeds - which `scripts/ledger_speculos.sh` does - the pair rules out a
///   device that ignores its seed and returns constants.
fn the_table_matches_the_device(_context: &ScenarioContext<'_>) -> ScenarioResult {
    require_table_is_populated()?;
    let seed = identify_seed()?;
    let other = match seed {
        SeedId::SpeculosDefault => SeedId::Alternate,
        SeedId::Alternate => SeedId::SpeculosDefault,
    };

    // Every row is checked before failing. A vector table that stops at the first mismatch tells you one row moved;
    // one that reports all of them tells you whether the derivation changed or a single row was mistyped, which is
    // the difference between a five minute and a five hour diagnosis.
    let mut problems = Vec::new();
    for vector in DERIVATION_VECTORS {
        let actual = ask_device(vector)?;
        if actual != vector.expected(seed) {
            problems.push(format!(
                "  {}\n    expected {}\n    device   {actual}",
                vector.name,
                vector.expected(seed)
            ));
        }
        if actual == vector.expected(other) {
            problems.push(format!(
                "  {}\n    answered the {} seed's value while loaded with the {} seed - either the harness started \
                 the wrong seed, or the device is not deriving from the seed at all",
                vector.name,
                other.name(),
                seed.name()
            ));
        }
    }

    require(problems.is_empty(), || {
        format!(
            "{} of {} vectors disagree with the device under the {} seed:\n{}",
            problems.len(),
            DERIVATION_VECTORS.len(),
            seed.name(),
            problems.join("\n")
        )
    })
}

/// Acceptance: the device answers the same value twice for the same inputs.
///
/// Derivation is meant to be a pure function of (seed, account, index, key type). If it were not - if some device
/// state or the RNG leaked into it - a wallet would not be able to find its own outputs after a restart. Cheap to
/// assert, and not implied by any single-shot vector comparison.
fn derivation_is_deterministic(_context: &ScenarioContext<'_>) -> ScenarioResult {
    require_table_is_populated()?;
    identify_seed()?;

    for vector in DERIVATION_VECTORS {
        let first = ask_device(vector)?;
        let second = ask_device(vector)?;
        require(first == second, || {
            format!(
                "'{}' is not deterministic on the device: {first} then {second}",
                vector.name
            )
        })?;
    }
    Ok(())
}

/// Acceptance: the account path element wraps at 2^32, because `make_bip32_path` accumulates it into a `u32`.
///
/// Kept out of the table because it asserts a *relationship* rather than a value: two accounts 2^32 apart address
/// the same key. See `vectors::ACCOUNT_WRAP_VECTOR` for why this sharp edge is worth pinning rather than quietly
/// tolerating - the host sends random `u64` accounts, and an SDK that started saturating instead of wrapping would
/// silently move which key a large account addresses.
fn the_account_wraps_at_u32(_context: &ScenarioContext<'_>) -> ScenarioResult {
    identify_seed()?;
    let (low, wrapping, branch) = ACCOUNT_WRAP_VECTOR;

    let from_low = ledger_get_public_key(low, 0, branch).context(|| format!("GetPublicKey for account {low}"))?;
    let from_wrapping =
        ledger_get_public_key(wrapping, 0, branch).context(|| format!("GetPublicKey for account {wrapping}"))?;

    require(from_low == from_wrapping, || {
        format!(
            "account {low} and account {wrapping} no longer collide - `make_bip32_path` has stopped wrapping, which \
             changes which key every large account addresses"
        )
    })
}
