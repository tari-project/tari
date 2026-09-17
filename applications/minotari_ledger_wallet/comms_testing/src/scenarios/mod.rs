// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The scenario library: one set of assertions, run by two frontends.
//!
//! # One library, two frontends, everything shared
//!
//! * `tests/speculos_scenarios.rs` runs every scenario against a Speculos simulator, unattended, with
//!   [`crate::approver::SpeculosApprover`] answering the one scenario that raises a review.
//! * `examples/ledger_demo.rs` runs **the same scenarios, with the same assertions** against real hardware, with
//!   [`crate::approver::HumanApprover`] answering that same scenario.
//!
//! A scenario declares whether it needs approval ([`Scenario::approval`]) and **nothing else varies between the two
//! frontends**. That is affordable for one reason: exactly one instruction in the whole application -
//! `GetOneSidedMetadataSignature` - puts anything in front of a human. Every other scenario here, including all of
//! the malformed-APDU probes and all of the nonce eviction probes, runs completely unattended on real hardware.
//!
//! # Why not `transaction_components::test_helpers`
//!
//! Because those build their fixtures through the key manager, and this suite exists to be independent of it. See
//! [`crate::fixtures`], which is where the inputs come from instead.
//!
//! # Happy paths go through the shipped accessors; rejections go over raw APDUs
//!
//! `minotari_ledger_wallet_comms::accessor_methods` is what the console wallet calls, and several of those methods
//! are more than a payload layout - they assemble chunk sequences, choose instructions, and parse replies. Every
//! scenario the device is meant to *accept* therefore drives the accessor, so that a regression in shipped code
//! fails this suite.
//!
//! Every scenario the device is meant to *refuse* has to go over [`crate::raw`] instead, because the accessors
//! mirror the device's rules and turn those requests away before they reach the wire - and the device's copy of
//! each rule is the one that counts.
//!
//! # Why a scenario returns an error instead of asserting
//!
//! A scenario is a function, not a test. `assert!` inside one would unwind out of whichever frontend called it, and
//! on the hardware frontend that means panicking with a review possibly still on the device and an APDU exchange
//! outstanding - the exact case [`crate::approver::HumanApprover::abandon`] exists to handle. Returning
//! [`ScenarioError`] lets each frontend decide how to report, and lets a frontend report *every* failing scenario
//! in one run rather than the first.
//!
//! # Coverage is mechanically enforced
//!
//! [`test::every_instruction_has_a_registered_scenario`] enumerates `Instruction` through an exhaustive `match`, in
//! the style of `test_instruction_conversion` in `common/src/common_types.rs`. Adding a variant to the instruction
//! set without giving it a scenario is a **compile error**, not a silently uncovered instruction.
//!
//! `AppSW` deliberately has no equivalent gate. Several of its variants - `KeyDeriveFail`, `RandomNonceFail`,
//! `KeyDeriveFromUniform` - are only reachable from device-internal failures that no host can provoke, so a
//! mechanical check would either be permanently red or would have to carry an exception list that nobody re-reads.
//! It is a reviewed checklist instead, restated in the pull request that adds each instruction.

pub mod crypto;
pub mod handshake;
pub mod legacy_nonce;
pub mod protocol;
pub mod stateful;
pub mod vectors;

use std::fmt;

use minotari_ledger_wallet_common::common_types::{AppSW, Instruction};

use crate::{approver::Approver, raw::RawReply};

/// Whether a scenario puts a review on the device's screen.
///
/// The only axis on which the simulator frontend and the hardware frontend are allowed to differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// Runs unattended. Every scenario here but one.
    NotNeeded,
    /// Raises a review, which the frontend's [`Approver`] answers.
    Required,
}

/// Why a scenario failed.
///
/// A plain message, because everything a reader needs is the sentence: what was asked, what came back, and what was
/// expected instead. Each constructor below is responsible for putting all three in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioError {
    pub message: String,
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ScenarioError {}

pub type ScenarioResult = Result<(), ScenarioError>;

/// One assertion, runnable by either frontend.
pub struct Scenario {
    /// What it checks, as a sentence fragment. This is what a frontend prints and what a JUnit entry is named
    /// after, so it has to be readable on its own.
    pub name: &'static str,
    /// Which `Instruction` variants this scenario exercises on the device.
    ///
    /// Not documentation: [`test::every_instruction_has_a_registered_scenario`] reads it, and an instruction that
    /// appears in no scenario's `covers` fails the build.
    pub covers: &'static [Instruction],
    pub approval: Approval,
    pub run: fn(&ScenarioContext<'_>) -> ScenarioResult,
}

impl fmt::Debug for Scenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scenario")
            .field("name", &self.name)
            .field("covers", &self.covers)
            .field("approval", &self.approval)
            .finish_non_exhaustive()
    }
}

/// A group of scenarios, which is also the unit a frontend reports as one test.
///
/// Grouping matters for more than tidiness: the device state these scenarios exercise - the ephemeral nonce store
/// and the script offset context - lives for as long as the application does, and `cargo nextest` gives every
/// `#[test]` its own process. Scenarios that have to observe one another's leftovers therefore have to be in one
/// module and run by one test.
pub struct ScenarioModule {
    pub name: &'static str,
    pub scenarios: &'static [Scenario],
}

/// Every scenario module, in the order a frontend should run them.
///
/// `handshake` is first and that is load bearing. Every other scenario assumes the ledger application has been
/// verified - `raw::send` deliberately does not verify, so that a malformed-APDU probe cannot interleave five
/// verification exchanges into the device state it was setting up - and `handshake` is what performs that
/// verification. It also has to run while `verify_ledger_application`'s process wide cache is still cold, which is
/// the only moment its concurrency assertion means anything.
pub const MODULES: &[ScenarioModule] = &[
    handshake::MODULE,
    vectors::MODULE,
    crypto::MODULE,
    protocol::MODULE,
    stateful::MODULE,
    legacy_nonce::MODULE,
];

/// Every scenario in every module.
pub fn all_scenarios() -> impl Iterator<Item = &'static Scenario> {
    MODULES.iter().flat_map(|module| module.scenarios.iter())
}

/// What a scenario is given: the approver for whichever frontend is running it, and nothing else.
///
/// Deliberately thin. Anything else in here would be a way for the two frontends to diverge, and the seed - the one
/// thing that looks like it belongs - is not here on purpose: [`vectors`] identifies the seed by asking the device,
/// so a scenario never has to be told which device it is talking to.
pub struct ScenarioContext<'a> {
    approver: &'a dyn Approver,
}

impl<'a> ScenarioContext<'a> {
    pub fn new(approver: &'a dyn Approver) -> Self {
        Self { approver }
    }

    pub fn approver(&self) -> &dyn Approver {
        self.approver
    }
}

/// Build a failure.
pub fn fail(message: impl Into<String>) -> ScenarioError {
    ScenarioError {
        message: message.into(),
    }
}

/// Fail unless `condition` holds, building the message only when it does not.
pub fn require(condition: bool, message: impl FnOnce() -> String) -> ScenarioResult {
    if condition { Ok(()) } else { Err(fail(message())) }
}

/// Turn any error into a [`ScenarioError`], saying what was being attempted.
///
/// An extension trait rather than a pile of `From` impls: the useful half of one of these failures is always the
/// *context* - "GetPublicKey for the sender offset key" - and a `From` impl cannot carry it.
pub trait WithContext<T> {
    fn context(self, what: impl FnOnce() -> String) -> Result<T, ScenarioError>;
}

impl<T, E: fmt::Display> WithContext<T> for Result<T, E> {
    fn context(self, what: impl FnOnce() -> String) -> Result<T, ScenarioError> {
        self.map_err(|e| fail(format!("{}: {e}", what())))
    }
}

/// Fail unless the device answered `9000`.
pub fn expect_ok(what: &str, reply: &RawReply) -> ScenarioResult {
    require(reply.is_ok(), || {
        format!(
            "{what} should have been accepted but the device answered {}",
            reply.describe_status()
        )
    })
}

/// Fail unless the device answered exactly `expected`.
///
/// Exact rather than "any failure" on purpose. "The device refused it" and "the device refused it *for the reason
/// the containment is written in terms of*" are different statements, and only the second one survives a
/// refactoring that moves a check somewhere it no longer covers the case the scenario is about. The legacy nonce
/// scenarios are the sharpest example: a `Spend` key branch has to be refused as `BadBranchKey`, and being refused
/// as `WrongApduLength` instead would mean the whitelist was never consulted.
pub fn expect_status(what: &str, reply: &RawReply, expected: AppSW) -> ScenarioResult {
    require(reply.status == expected as u16, || {
        format!(
            "{what} should have been refused with {expected:?} ({:#06x}) but the device answered {}",
            expected as u16,
            reply.describe_status()
        )
    })
}

/// Fail unless the device answered exactly the raw status word `expected`.
///
/// For the two status words the SDK owns rather than the application, which [`AppSW`] has no name for.
pub fn expect_raw_status(what: &str, reply: &RawReply, expected: u16) -> ScenarioResult {
    require(reply.status == expected, || {
        format!(
            "{what} should have been refused with {expected:#06x} but the device answered {}",
            reply.describe_status()
        )
    })
}

/// Fail if the device accepted something it was supposed to refuse, without saying which refusal was expected.
///
/// Used only where the device has a genuine choice of refusals and the scenario's point is that it refused at all;
/// prefer [`expect_status`] everywhere else.
pub fn expect_refused(what: &str, reply: &RawReply) -> ScenarioResult {
    require(!reply.is_ok(), || {
        format!(
            "{what} should have been refused, but the device accepted it and returned {} bytes",
            reply.data.len()
        )
    })
}

#[cfg(test)]
mod test {
    use super::*;

    /// Every `Instruction` variant is exercised by at least one scenario - and adding a variant without one is a
    /// **compile error**, not a red test.
    ///
    /// The exhaustive `match` is what does that, and it is the same shape as `test_instruction_conversion` in
    /// `minotari_ledger_wallet_common::common_types`. A new variant added to the enum makes the match non
    /// exhaustive, so this file stops compiling and whoever added the instruction is told where to register its
    /// scenario. A `for` loop over a hand written list would only have gone red, and only if somebody remembered to
    /// add it to the list.
    ///
    /// The byte values are asserted alongside, so that this list and the wire protocol cannot drift apart either.
    #[test]
    fn every_instruction_has_a_registered_scenario() {
        let mappings = [
            (0x01, Instruction::GetVersion),
            (0x02, Instruction::GetAppName),
            (0x03, Instruction::GetPublicSpendKey),
            (0x04, Instruction::GetPublicKey),
            (0x05, Instruction::GetScriptSignatureDerived),
            (0x06, Instruction::GetScriptOffset),
            (0x07, Instruction::GetViewKey),
            (0x08, Instruction::GetDHSharedSecret),
            (0x09, Instruction::GetRawSchnorrSignature),
            (0x10, Instruction::GetScriptSchnorrSignature),
            (0x11, Instruction::GetOneSidedMetadataSignature),
            (0x12, Instruction::GetScriptSignatureManaged),
            (0x13, Instruction::GenerateEphemeralNonce),
            (0x14, Instruction::GetRawSchnorrSignatureLegacyNonce),
        ];

        for (expected_byte, instruction) in mappings {
            assert_eq!(instruction.as_byte(), expected_byte);
            // One arm per variant. Do not collapse these into a wildcard: the wildcard is exactly what would let a
            // new instruction ship with no scenario and no complaint.
            match instruction {
                Instruction::GetVersion => assert_covered(instruction),
                Instruction::GetAppName => assert_covered(instruction),
                Instruction::GetPublicSpendKey => assert_covered(instruction),
                Instruction::GetPublicKey => assert_covered(instruction),
                Instruction::GetScriptSignatureDerived => assert_covered(instruction),
                Instruction::GetScriptOffset => assert_covered(instruction),
                Instruction::GetViewKey => assert_covered(instruction),
                Instruction::GetDHSharedSecret => assert_covered(instruction),
                Instruction::GetRawSchnorrSignature => assert_covered(instruction),
                Instruction::GetScriptSchnorrSignature => assert_covered(instruction),
                Instruction::GetOneSidedMetadataSignature => assert_covered(instruction),
                Instruction::GetScriptSignatureManaged => assert_covered(instruction),
                Instruction::GenerateEphemeralNonce => assert_covered(instruction),
                Instruction::GetRawSchnorrSignatureLegacyNonce => assert_covered(instruction),
            }
        }

        assert_eq!(
            mappings.len(),
            14,
            "the instruction set changed size; add the new variant to the match above and give it a scenario"
        );
    }

    fn assert_covered(instruction: Instruction) {
        let covering: Vec<&str> = all_scenarios()
            .filter(|scenario| scenario.covers.contains(&instruction))
            .map(|scenario| scenario.name)
            .collect();
        assert!(
            !covering.is_empty(),
            "{instruction:?} has no scenario. Every instruction the device serves has to be exercised by at least one \
             scenario in `scenarios`; add one and list {instruction:?} in its `covers`."
        );
    }

    /// A scenario that covers nothing would satisfy nothing and would quietly stop counting towards the gate above
    /// if its instruction list were ever emptied by a bad merge.
    #[test]
    fn every_scenario_covers_at_least_one_instruction() {
        for scenario in all_scenarios() {
            assert!(
                !scenario.covers.is_empty(),
                "'{}' does not say which instructions it covers",
                scenario.name
            );
        }
    }

    /// Names are what a frontend prints and what the JUnit report is keyed on, so a duplicate would make two
    /// different failures indistinguishable.
    #[test]
    fn every_scenario_name_is_unique() {
        let mut names: Vec<&str> = all_scenarios().map(|scenario| scenario.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two scenarios share a name");

        let mut modules: Vec<&str> = MODULES.iter().map(|module| module.name).collect();
        let count = modules.len();
        modules.sort_unstable();
        modules.dedup();
        assert_eq!(modules.len(), count, "two scenario modules share a name");
    }

    /// The suite must not be empty, and neither must any module in it.
    ///
    /// Every frontend is a `for` loop, and a `for` loop over an empty slice passes. Without this, a truncated
    /// module list would let the whole device suite report green against a live simulator having asserted nothing -
    /// the same silence `vectors::EXPECTED_VECTOR_COUNT` exists to prevent, arriving one level up.
    #[test]
    fn no_module_is_empty() {
        assert!(!MODULES.is_empty(), "there are no scenario modules");
        for module in MODULES {
            assert!(
                !module.scenarios.is_empty(),
                "the '{}' module has no scenarios",
                module.name
            );
        }
        assert!(all_scenarios().count() >= MODULES.len());
    }

    /// Exactly one scenario raises a review.
    ///
    /// This is the premise of locked decision 1 - one scenario library driving both frontends is affordable
    /// *because* only `GetOneSidedMetadataSignature` shows a screen - so it is asserted rather than assumed. A
    /// second approval scenario is not forbidden, but it doubles what a human has to do for every hardware run, so
    /// it should be a decision somebody made on purpose rather than one that arrived with a merge.
    #[test]
    fn exactly_one_scenario_needs_approval() {
        let approving: Vec<&str> = all_scenarios()
            .filter(|scenario| scenario.approval == Approval::Required)
            .map(|scenario| scenario.name)
            .collect();
        assert_eq!(
            approving.len(),
            1,
            "expected exactly one approval scenario, found {approving:?}. Every one of these has to be answered by \
             hand on the hardware frontend."
        );
    }

    /// An approval scenario is one that reaches the only instruction with a review screen.
    ///
    /// One direction only, and the asymmetry is the interesting part. Every scenario that declares
    /// [`Approval::Required`] must be a `GetOneSidedMetadataSignature` scenario, because no other handler in the
    /// application draws anything - a scenario that asked for approval on any other instruction would leave the
    /// hardware frontend asking an operator about a screen that will never appear.
    ///
    /// The converse does **not** hold, and asserting it was a mistake worth recording: `protocol`'s length probe
    /// sends a `GetOneSidedMetadataSignature` that is one byte below the handler's minimum, which is refused before
    /// the review is built. Covering the instruction and *completing* it are different things, and only the second
    /// needs a human. Aiming that probe at this instruction is deliberate for exactly that reason - see
    /// [`super::protocol`].
    #[test]
    fn every_approval_scenario_is_a_review_scenario() {
        for scenario in all_scenarios().filter(|scenario| scenario.approval == Approval::Required) {
            assert!(
                scenario.covers.contains(&Instruction::GetOneSidedMetadataSignature),
                "'{}' asks for approval, but GetOneSidedMetadataSignature is the only instruction that shows a \
                 screen, so nothing would ever appear for the operator to answer",
                scenario.name
            );
        }
    }

    /// The status helpers say what they expected and what they got, because a bare "assertion failed" on a status
    /// word is the least actionable failure this suite can produce.
    #[test]
    fn the_status_helpers_report_both_sides() {
        let refused = RawReply {
            status: AppSW::WrongApduLength as u16,
            data: Vec::new(),
        };
        let message = expect_status("a probe", &refused, AppSW::BadBranchKey)
            .expect_err("the status words differ")
            .to_string();
        assert!(message.contains("a probe"), "{message}");
        assert!(message.contains("BadBranchKey"), "{message}");
        assert!(message.contains("WrongApduLength"), "{message}");

        assert!(expect_status("a probe", &refused, AppSW::WrongApduLength).is_ok());
        assert!(expect_refused("a probe", &refused).is_ok());
        assert!(expect_ok("a probe", &refused).is_err());

        let accepted = RawReply {
            status: AppSW::Ok as u16,
            data: vec![1, 2, 3],
        };
        assert!(expect_ok("a probe", &accepted).is_ok());
        let message = expect_refused("a probe", &accepted)
            .expect_err("the device accepted it")
            .to_string();
        assert!(message.contains("3 bytes"), "{message}");
    }

    #[test]
    fn context_keeps_what_was_being_attempted() {
        let result: Result<(), String> = Err("the device went away".to_string());
        let message = result
            .context(|| "GetPublicKey for the sender offset key".to_string())
            .expect_err("still an error")
            .to_string();
        assert!(message.contains("GetPublicKey for the sender offset key"), "{message}");
        assert!(message.contains("the device went away"), "{message}");
    }
}
