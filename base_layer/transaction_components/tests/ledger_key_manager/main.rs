// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The key manager's Ledger wrappers in `src/key_manager/manager.rs`, driven against a Speculos simulator.
//!
//! # What this is, and what it deliberately is not
//!
//! The console wallet was only ever the vehicle; `manager.rs` was always the target. Every `ledger_*_wrapper` there
//! is a thin translation from key manager concepts - key ids, reserved nonces, wallet outputs - into
//! `minotari_ledger_wallet_comms::accessor_methods` calls and back, and that translation is where the last two
//! Ledger fixes landed. So these tests build a ledger-mode
//! [`KeyManager`](tari_transaction_components::key_manager::KeyManager) exactly the way `applications/
//! minotari_console_wallet/src/init/mod.rs` does - `GetPublicSpendKey`, `GetViewKey`, then `LedgerWallet::new(account,
//! network, public_alpha, view_key)` - and drive it through its public interface.
//!
//! What that gives up, on purpose and not by oversight:
//!
//! * **The console wallet's init path and its UI wiring.** Those are thin glue, and testing them would need a shipped
//!   binary that could be pointed at a simulator, which is exactly what the `comms_testing` crate exists to make
//!   impossible. A test that seems to need a redirectable binary is the wrong test.
//! * **Cucumber, a base node, a miner, LocalNet.** None of the wrappers talks to any of them.
//!
//! Unlike the Spec 4 scenario library in `comms_testing` - which builds its inputs by hand precisely so that a key
//! manager change cannot turn it red - this suite goes *through* the key manager and `crate::test_helpers` on
//! purpose. Here the key manager is the thing under test. What it reuses from `comms_testing` is the apparatus -
//! the transport, the seed gate, the [`Approver`](minotari_ledger_wallet_comms_testing::approver::Approver), the
//! review expectations - and none of the scenarios, which it does not fork.
//!
//! # Delivery order is risk order
//!
//! The modules are named so that they run, and report, in the order the risk was ranked:
//!
//! 1. [`g1_script_offset`] - chunked, stateful, and one unblinded reply away from the wallet's spend key.
//! 2. [`g2_ephemeral_nonce`] - reserve now, sign later; the area of the last two fixes.
//! 3. [`g3_legacy_nonce`] - the deprecated host-indexed nonce, and the `(LedgerKey, LedgerKey)` arm.
//! 4. [`g4_one_sided_metadata`] - the only instruction with a review screen, so the only one needing the approver.
//! 5. [`g5_the_rest`] - script signatures, script Schnorr signatures, public keys and Diffie-Hellman.
//!
//! # How these are gated, and why it is not `#[ignore]`
//!
//! Everything below this doc comment only exists when the crate is compiled with `--cfg tari_ledger_speculos`,
//! which nothing sets except `scripts/ledger_speculos.sh`. Without it this file compiles to an empty test binary,
//! so `cargo ci-test` - with no simulator anywhere - runs nothing here and cannot hang on a socket. With it, every
//! test runs on every pull request, in the `ledger speculos tests` job, on each model and seed that job covers.
//!
//! The same cfg is what makes `minotari_ledger_wallet_comms_testing` a dev-dependency at all; see the comment on
//! the `cfg(tari_ledger_speculos)` table in this crate's `Cargo.toml` for why a cargo feature could not do this
//! and why the harness must not be an unconditional dev-dependency.
//!
//! There is no `#[ignore]` here. The `comms_testing` suite gates on it and runs `--run-ignored all`; this suite
//! cannot, because `#[ignore]` in this crate would be indistinguishable from a test parked on a bug, and those
//! must carry an issue link. Nothing in this file is parked.
//!
//! ```text
//! ./scripts/ledger_speculos.sh test          # both suites, every model and seed
//!
//! ./scripts/ledger_speculos.sh up stax default
//! SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address stax default) \
//! SPECULOS_API_ADDRESS=$(./scripts/ledger_speculos.sh api-address stax default) \
//! SPECULOS_MODEL=stax SPECULOS_SEED_ID=default \
//! RUSTFLAGS="--cfg tari_ledger_speculos" CARGO_TARGET_DIR=target/ledger-speculos \
//!   cargo test -p tari_transaction_components --features ledger --test ledger_key_manager -- --test-threads=1
//! ```
//!
//! # One device, one test at a time, and always back home
//!
//! Speculos is one device. Under `cargo nextest` every test is its own process and `test-threads = 1` in the
//! `ledger-speculos` profile of `.config/nextest.toml` is what serialises them; under `cargo test` they are threads
//! of one process and the mutex in [`harness`] does. Every test starts by asserting the device is at its home screen
//! and ends by waiting for it to return there, so a test that strands the device is blamed for it rather than the
//! next one along - and the simulator is never restarted in between, because the nonce store and the script offset
//! context are precisely the state worth keeping.
//!
//! No test here sleeps or retries. Every wait is on the device drawing something, through the approver.

#![cfg(tari_ledger_speculos)]

// Without `ledger` the key manager refuses a ledger wallet type outright, and every test would fail in its first
// line with an error about a feature rather than about a device. Said once, here, at compile time instead.
#[cfg(not(feature = "ledger"))]
compile_error!(
    "the ledger key manager tests need `--features ledger`: `cargo test -p tari_transaction_components --features \
     ledger --test ledger_key_manager`, or `./scripts/ledger_speculos.sh test`"
);

mod harness;

mod g1_script_offset;
mod g2_ephemeral_nonce;
mod g3_legacy_nonce;
mod g4_one_sided_metadata;
mod g5_the_rest;
