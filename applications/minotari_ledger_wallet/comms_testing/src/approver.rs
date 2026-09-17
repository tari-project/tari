// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Answering the device's review screen, and asserting what it said before answering it.
//!
//! # One trait, two frontends, one scenario library
//!
//! [`Approver`] has exactly two implementations:
//!
//! * [`SpeculosApprover`] presses the buttons or taps the screen of a simulator, and asserts the field text it reads
//!   back out of Speculos' event stream.
//! * [`HumanApprover`] prints the expected fields, asks an operator to confirm the device in their hand shows *exactly*
//!   those, and then waits while they press the button themselves. **The operator is the assertion oracle.** There is
//!   no other way to assert a real device's screen, and pretending otherwise - by asserting nothing on hardware, say -
//!   would mean the hardware path silently tested less than the simulator path.
//!
//! The consequence is deliberate and is the point of the trait: **a scenario that cannot say what it expects on
//! screen cannot run on hardware.** That forces every scenario to state its expectations declaratively, as an
//! [`ExpectedReview`], which is what lets one scenario library drive both frontends instead of two libraries that
//! agree on the day they are written and never again.
//!
//! # Driving, not Ragger
//!
//! Ragger is Ledger's Python test framework, and it is not used here. Introducing a Python stack for one model's
//! touchscreen would re-split the scenario library that this trait exists to unify: Rust scenarios for
//! `nanosplus`, Python scenarios for `stax`, and two months later they no longer test the same thing. The touch
//! driving needed for one review screen is [`SpeculosApi::touch`] and a text anchor.
//!
//! # Tap by text anchor, not by coordinate table
//!
//! The Step 0 spike asked whether Speculos' event stream carries widget geometry for NBGL. It does: every event
//! carries `x`, `y`, `w` and `h`. So every **button** this harness presses is found by matching its text -
//! `"Hold to sign"`, `"Reject"`, `"Yes, reject"` - and tapped at the centre of the rectangle Speculos reports for
//! that text. An SDK release that moves a button does not need a test change.
//!
//! The one exception is the page **swipe**, which is a movement across the screen rather than a press on a widget
//! and so has nothing to anchor to. That needs the screen's dimensions, which is the only per-model coordinate
//! constant in this harness - see [`nbgl_swipe`], which is annotated with the SDK versions it was measured
//! against.
//!
//! # Event driven, and no retries
//!
//! Every wait in here blocks on [`EventStream::next_event`], which returns when the device draws and not before.
//! There is no `sleep` and no retry loop, and neither is an oversight:
//!
//! * A `sleep` long enough to be reliable on a loaded runner is paid on every screen of every scenario, and one short
//!   enough to be quick is a race. Both are worse than blocking on the thing you are actually waiting for.
//! * A retry is worse still. The device state this suite exists to test - the ephemeral nonce store, the script offset
//!   context - is exactly the state a second attempt disturbs. A test that goes green on a retry has usually destroyed
//!   the evidence that it was red. **A test that needs a retry is a bug report.**
//!
//! When a wait does run out, [`ReviewError::Timeout`] carries the full event log and the last screen, because a
//! timeout with no diagnostics is the single failure mode most likely to make people give up on a suite.

use std::{
    env,
    fmt,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    review::{DeviceModel, ExpectedReview, UiToolkit},
    speculos_api::{ApiError, Button, EventStream, ScreenText, SpeculosApi, Touch, joined},
};

/// Which way to answer the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Approve,
    Reject,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Approve => write!(f, "approve"),
            Outcome::Reject => write!(f, "reject"),
        }
    }
}

/// Answer the device's review screen, having first checked it says what it is supposed to say.
///
/// Both methods assert **before** they answer. On a mismatch the review is rejected rather than left on screen,
/// so a failing scenario hands the next scenario a device at its home screen rather than one wedged mid-review -
/// which matters a great deal given that this suite deliberately does not restart the simulator between
/// scenarios.
pub trait Approver {
    /// Check the review against `expected` and approve it.
    fn expect_and_approve(&self, expected: &ExpectedReview) -> Result<(), ReviewError>;

    /// Check the review against `expected` and reject it.
    fn expect_and_reject(&self, expected: &ExpectedReview) -> Result<(), ReviewError>;
}

/// Everything the harness saw, for when something did not go as expected.
#[derive(Debug, Clone, Default)]
pub struct Transcript {
    /// The text of each screen of the review, in the order it appeared.
    pub screens: Vec<String>,
    /// Every text the device drew since the scenario started, across all screens.
    pub event_log: Vec<ScreenText>,
    /// What was on the screen when things stopped going to plan.
    pub last_screen: Vec<ScreenText>,
    /// Where the last screen's PNG was written, if it could be.
    pub screenshot: Option<PathBuf>,
}

impl fmt::Display for Transcript {
    /// # Device text is always written with `{:?}`
    ///
    /// Everything in here is a string the *device* drew, reconstructed from Speculos' JSON with no constraint on
    /// what characters it may contain, and this `Display` ends up in an assertion message - so in a terminal, and
    /// in the JUnit `<failure>` body `scripts/ledger_speculos.sh` copies into an artifact directory for CI to
    /// upload. Written plainly, a device could put terminal escapes in front of whoever reads that.
    ///
    /// `{:?}` escapes them and is free, and every site in this crate that quotes device text does the same:
    /// [`ScreenText`]'s own `Display`, [`ReviewError::NotAtHome`], the three mismatch messages in
    /// [`ExpectedReview::check`], and the `handshake` scenarios' app name and version. The impact is low - it takes
    /// a device you chose to point this at - but the sink is wide enough that fixing it narrowly would just leave
    /// the next one open, which is why this lists them rather than claiming to be the last.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  Screens seen ({}):", self.screens.len())?;
        for (index, screen) in self.screens.iter().enumerate() {
            writeln!(f, "    [{index}] {screen:?}")?;
        }
        writeln!(f, "  Full event log ({} events):", self.event_log.len())?;
        for event in &self.event_log {
            writeln!(f, "    {event}")?;
        }
        writeln!(f, "  Last screen ({} events):", self.last_screen.len())?;
        for event in &self.last_screen {
            writeln!(f, "    {event}")?;
        }
        writeln!(f, "    joined: {:?}", joined(&self.last_screen))?;
        match &self.screenshot {
            Some(path) => writeln!(f, "  Last screen as a PNG: {}", path.display()),
            None => writeln!(f, "  Last screen as a PNG: could not be written"),
        }
    }
}

/// Why a review could not be answered.
#[derive(Debug)]
pub enum ReviewError {
    /// Speculos' control API could not be reached, or said something unexpected.
    Api(ApiError),
    /// The device did not do anything for long enough that waiting further was pointless.
    Timeout {
        waiting_for: String,
        after: Duration,
        transcript: Box<Transcript>,
    },
    /// The device showed something other than what the scenario said it would.
    Mismatch {
        problems: Vec<String>,
        transcript: Box<Transcript>,
    },
    /// A scenario tried to start while the device was showing something other than its home screen.
    NotAtHome { screen: String },
    /// A human said no, or could not be asked.
    Operator(String),
}

impl fmt::Display for ReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewError::Api(e) => write!(f, "Speculos' control API: {e}"),
            ReviewError::Timeout {
                waiting_for,
                after,
                transcript,
            } => {
                writeln!(
                    f,
                    "Timed out after {after:?} waiting for {waiting_for}.\n\nEverything the device drew:"
                )?;
                write!(f, "{transcript}")
            },
            ReviewError::Mismatch { problems, transcript } => {
                writeln!(f, "The device did not show what the scenario expected:")?;
                for problem in problems {
                    writeln!(f, "  - {problem}")?;
                }
                writeln!(
                    f,
                    "\nThe review was rejected, so the device is back at its home screen.\n"
                )?;
                write!(f, "{transcript}")
            },
            ReviewError::NotAtHome { screen } => write!(
                f,
                "The device is not at its home screen, it is showing {screen:?}. A scenario that starts from an \
                 unknown screen cannot know what it is looking at. Note that the harness deliberately does not \
                 restart the simulator between scenarios - see the crate docs - so this usually means an earlier \
                 scenario left a review open."
            ),
            ReviewError::Operator(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ReviewError {}

impl From<ApiError> for ReviewError {
    fn from(e: ApiError) -> Self {
        ReviewError::Api(e)
    }
}

/// How long to wait for the device to draw something before giving up and dumping everything.
///
/// Comfortably shorter than [`crate::DEFAULT_READ_TIMEOUT`], the bound on the APDU exchange this is driving, so
/// that a stuck review is reported by the side that can say *what was on the screen* rather than by the side that
/// can only say the device went quiet.
pub const DEFAULT_REVIEW_TIMEOUT: Duration = Duration::from_secs(60);

/// Which model is running, from `SPECULOS_MODEL`.
pub const SPECULOS_MODEL: &str = "SPECULOS_MODEL";

/// The model the simulator is running.
///
/// Unlike [`crate::simulator::SPECULOS_APDU_ADDRESS`], an unset value here is allowed to default, and the reason
/// is worth stating because the crate is otherwise strict about this: a wrong model cannot make a test pass. It
/// selects buttons instead of taps or the wrong lossy-text adjustment, and every one of those ends in a timeout
/// or a mismatch with the screen printed next to it. The failure mode a default has to be protected against is
/// silence, and there is none available here.
pub fn model() -> Result<DeviceModel, ReviewError> {
    model_from(env::var(SPECULOS_MODEL).ok().as_deref())
}

fn model_from(value: Option<&str>) -> Result<DeviceModel, ReviewError> {
    match value.map(str::trim) {
        None => Ok(DeviceModel::NanoSPlus),
        Some(name) => DeviceModel::from_name(name).ok_or_else(|| {
            ReviewError::Api(ApiError(format!(
                "{SPECULOS_MODEL} is '{name}', which is not one of {:?}",
                DeviceModel::ALL.map(DeviceModel::name)
            )))
        }),
    }
}

/// The text every screen of the device's home menu has in it, on both toolkits.
///
/// `nanosplus` draws `MinoTari` and `Wallet` on two lines; `stax` draws `MinoTari Wallet` and a description. No
/// screen of the review contains it, which is what makes it usable as the "nothing is in progress" marker.
///
/// # Every anchor below is compared as the toolkit *reports* it
///
/// A BAGL screen does not reach this harness as the string the application drew; it reaches it as Speculos'
/// reconstruction of the pixels, which drops `S` and turns `I` into `l` - see [`crate::review::as_bagl_reports`].
/// Comparing a raw anchor against that works only while the anchor happens to contain neither character, which is
/// true of all of these today and is a property of the words Ledger chose rather than of anything here. Every
/// comparison therefore goes through [`SpeculosApprover::reported`], and
/// `the_anchors_survive_bagl_reporting` asserts the assumption rather than relying on it.
const HOME_ANCHOR: &str = "MinoTari";

/// The text the device draws on the last page of an NBGL review, from the handler's
/// `.titles(_, _, "Sign transaction\nto send")`.
const NBGL_FINISH_ANCHOR: &str = "Sign transaction";
/// The NBGL widget that approves, from the SDK's hold-to-sign footer.
const NBGL_APPROVE_ANCHOR: &str = "Hold to sign";
/// The NBGL widget that starts a rejection, present on every page of the review.
const NBGL_REJECT_ANCHOR: &str = "Reject";
/// The NBGL widget that confirms a rejection, on the dialog the previous one opens.
const NBGL_CONFIRM_REJECT_ANCHOR: &str = "Yes, reject";

/// The BAGL page that approves, from the handler's `MultiFieldReview::new(.., "Approve", .., "Reject", ..)`.
const BAGL_APPROVE_ANCHOR: &str = "Approve";
/// The BAGL page that rejects, one page to the right of the approve page.
const BAGL_REJECT_ANCHOR: &str = "Reject";

/// Where to swipe, for the one gesture that cannot be aimed at a widget.
///
/// **Measured against `ledger_device_sdk = "=1.35.0"` and `ledger_secure_sdk_sys = "=1.16.3"`** - the versions
/// pinned in `applications/minotari_ledger_wallet/wallet/Cargo.toml` - and against the Speculos image digest
/// pinned in `scripts/ledger_speculos.sh`. These are screen dimensions rather than widget positions, so they move
/// only when Ledger ships a new physical device, but the annotation is here because an un-annotated coordinate
/// table is a trap for whoever bumps the SDK next.
///
/// A swipe is horizontal, across the middle of the screen, from four fifths of the way across to one fifth.
fn nbgl_swipe(model: DeviceModel) -> Touch {
    let (width, height) = match model {
        // Stax is 400x672, Flex is 480x600.
        DeviceModel::Stax => (400, 672),
        DeviceModel::Flex => (480, 600),
        // Not reachable - BAGL models never swipe - but a plausible value beats a panic if it ever is.
        DeviceModel::NanoSPlus | DeviceModel::NanoX => (128, 64),
    };
    Touch::Swipe {
        from: (width * 4 / 5, height / 2),
        to: (width / 5, height / 2),
    }
}

/// An [`Approver`] that drives a running Speculos simulator.
pub struct SpeculosApprover {
    api: SpeculosApi,
    model: DeviceModel,
    timeout: Duration,
}

impl SpeculosApprover {
    pub fn new(api: SpeculosApi, model: DeviceModel) -> Self {
        Self {
            api,
            model,
            timeout: DEFAULT_REVIEW_TIMEOUT,
        }
    }

    /// An approver for the simulator named by `SPECULOS_API_ADDRESS`, running the model named by `SPECULOS_MODEL`.
    pub fn from_env() -> Result<Self, ReviewError> {
        Ok(Self::new(SpeculosApi::from_env()?, model()?))
    }

    /// Change the deadline. Exists so that the timeout path itself can be tested in seconds rather than a minute.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub const fn model(&self) -> DeviceModel {
        self.model
    }

    pub fn api(&self) -> &SpeculosApi {
        &self.api
    }

    /// Start a scenario: fail unless the device is sitting at its home screen, and begin a fresh diagnostic log.
    ///
    /// # Why a home assertion rather than a restart
    ///
    /// The simulator is **not** restarted between scenarios. Restarting would be the easy way to guarantee a known
    /// starting state, and it would also delete the only state worth testing: the ephemeral nonce store
    /// (`EphemeralNonceCtx`, eight slots, alive for the life of the application) and the script offset context
    /// (`ScriptOffsetCtx`, accumulated across chunks and reset by any interleaved instruction). A suite that tears
    /// the device down between scenarios cannot see either of them, so it cannot test the thing that most needs
    /// testing. This assertion is what replaces the restart.
    ///
    /// # Why it waits rather than samples
    ///
    /// An instruction's APDU reply goes out before the device redraws its home screen, so a scenario that has just
    /// finished one instruction can reach this line before the redraw has happened. Sampling the screen once would
    /// make that a race that fails perhaps one run in fifty - which is the kind of failure that gets a suite
    /// labelled flaky and then ignored. It blocks on the event stream instead, so it returns the moment the device
    /// is home and fails, with what *is* on the screen, only when it never gets there.
    ///
    /// # The diagnostic log
    ///
    /// The event log is cleared here, and nowhere else, so that a failure anywhere in the scenario dumps
    /// everything the device drew **during that scenario** rather than everything since the simulator started.
    pub fn expect_home(&self) -> Result<(), ReviewError> {
        self.api.clear_event_log()?;
        self.wait_for_home()
    }

    /// Wait until the device is at its home screen, without starting a new diagnostic log.
    ///
    /// This is [`Self::expect_home`] without the scenario boundary, for asserting that an instruction left the
    /// device where it found it. Keeping the log is the point: a scenario that ends by checking the device came
    /// home should not, in doing so, throw away the record of what it did on the way.
    pub fn wait_for_home(&self) -> Result<(), ReviewError> {
        let mut stream = self.api.open_event_stream()?;
        let deadline = self.fresh_deadline();
        let home = self.reported(HOME_ANCHOR);
        loop {
            let screen = joined(&self.api.current_screen()?);
            if screen.contains(home.as_str()) {
                return Ok(());
            }
            if stream.next_event(deadline)?.is_none() {
                return Err(ReviewError::NotAtHome { screen });
            }
        }
    }

    fn toolkit(&self) -> UiToolkit {
        self.model.toolkit()
    }

    /// An anchor as Speculos will report it on this model, which is what the screen text has to be compared with.
    fn reported(&self, text: &str) -> String {
        self.toolkit().as_reported(text)
    }

    /// Walk the review from wherever the device is now to its final page, collecting what it showed.
    fn run(&self, expected: &ExpectedReview, outcome: Outcome) -> Result<(), ReviewError> {
        // Subscribe before anything else, so that a review drawn between now and the first read is still waited on
        // rather than missed. The diagnostic log is deliberately *not* cleared here - it is cleared once per
        // scenario in `expect_home`, so that a dump covers the whole scenario and not just the review.
        let mut stream = self.api.open_event_stream()?;

        let deadline = self.fresh_deadline();
        let home = self.reported(HOME_ANCHOR);
        let mut screens: Vec<String> = Vec::new();
        let mut current: Option<String> = None;
        let mut advanced_on_this_screen = false;

        let final_screen = loop {
            let texts = self.api.current_screen()?;
            let text = joined(&texts);

            // Speculos never forwards a screen clear to a stream subscriber, so boundaries have to be inferred:
            // within one screen the reported text only ever grows at the end, so anything that is not an extension
            // of what we had is a new screen.
            match &current {
                Some(previous) if previous.is_empty() || text.starts_with(previous.as_str()) => {},
                Some(previous) => {
                    // The home screen is not part of the review, and recording it would put the application's
                    // description into the text the assertions run against.
                    if !previous.contains(home.as_str()) {
                        screens.push(previous.clone());
                    }
                    advanced_on_this_screen = false;
                },
                None => advanced_on_this_screen = false,
            }
            current = Some(text.clone());

            if text.is_empty() {
                // A screen that has been cleared but not yet drawn on. Advancing here would be a button press at a
                // page the harness has not seen, and on the approve page that press moves to reject and then
                // wedges - so the one safe thing to do with a blank screen is wait for it to say something.
                self.wait(&mut stream, deadline, "the device to draw something", &screens)?;
                continue;
            }

            if text.contains(home.as_str()) {
                // The instruction has not reached its review yet. `verify_ledger_application` alone is four
                // exchanges, none of which draws anything.
                self.wait(&mut stream, deadline, "the review to appear", &screens)?;
                continue;
            }

            if self.is_final_page(&text) {
                break text;
            }

            // One advance per screen. A screen is still being drawn when its first event arrives, and the device
            // does not read the button queue until it has finished drawing, so advancing early is safe - but
            // advancing again as the rest of the screen arrives would skip a page.
            if !advanced_on_this_screen {
                self.advance()?;
                advanced_on_this_screen = true;
            }
            self.wait(&mut stream, deadline, "the next page of the review", &screens)?;
        };

        screens.push(final_screen);

        // Assert before answering. If the device is showing the wrong thing, the last thing to do is approve it.
        if let Err(problems) = expected.check(self.toolkit(), &screens) {
            // Reject, so that the device is left at its home screen for the next scenario rather than wedged on a
            // review nobody answered. A failing assertion must not also break everything after it.
            //
            // On a **fresh** budget, not the review walk's. A mismatch found on the last page of a slow review
            // arrives with the original deadline already spent, and cleanup on a spent deadline is no cleanup at
            // all: `wait_for_widget` returns immediately, the review stays open, the outstanding APDU blocks until
            // its own timeout, and every scenario after this one then fails its home assertion - which is the
            // precise cascade the two lines above exist to prevent.
            let cleanup_deadline = self.fresh_deadline();
            let rejected = self.reject(&mut stream, cleanup_deadline, &screens);
            let mut transcript = self.transcript(&screens);
            if let Err(e) = rejected {
                transcript
                    .screens
                    .push(format!("<the review could not be rejected afterwards: {e}>"));
            }
            return Err(ReviewError::Mismatch {
                problems,
                transcript: Box::new(transcript),
            });
        }

        match outcome {
            Outcome::Approve => self.approve(&mut stream, deadline, &screens),
            Outcome::Reject => self
                .reject(&mut stream, deadline, &screens)
                .map_err(|e| self.dress(e, &screens)),
        }
    }

    /// A deadline `self.timeout` from now.
    fn fresh_deadline(&self) -> Instant {
        Instant::now().checked_add(self.timeout).unwrap_or_else(Instant::now)
    }

    fn is_final_page(&self, text: &str) -> bool {
        let anchor = match self.toolkit() {
            UiToolkit::Bagl => BAGL_APPROVE_ANCHOR,
            UiToolkit::Nbgl => NBGL_FINISH_ANCHOR,
        };
        text.contains(self.reported(anchor).as_str())
    }

    fn advance(&self) -> Result<(), ApiError> {
        match self.toolkit() {
            UiToolkit::Bagl => self.api.press_button(Button::Right),
            UiToolkit::Nbgl => self.api.touch(nbgl_swipe(self.model)),
        }
    }

    fn approve(&self, stream: &mut EventStream, deadline: Instant, screens: &[String]) -> Result<(), ReviewError> {
        match self.toolkit() {
            UiToolkit::Bagl => Ok(self.api.press_button(Button::Both)?),
            UiToolkit::Nbgl => {
                // Hold to sign is a *hold*: the finger has to stay on the glass until the progress ring fills.
                //
                // How long that takes is not this harness's business to know. Speculos will hold for a fixed
                // number of seconds if asked (`delay` on a `press-and-release`), and that was the obvious thing to
                // do and is wrong twice over: it is a sleep by another name, and the number is an SDK constant
                // that nothing here can see, so it would be a guess that silently becomes too short on the day
                // Ledger lengthens the animation.
                //
                // So: press, and hold until the **screen stops showing the hold button**. Note "until the screen
                // changes", not "until the next event arrives" - the two are not the same, and the difference cost
                // an afternoon. Events are queued in the stream, so at the moment the finger goes down there are
                // usually several of this very page's own draws still unread; waiting for "an event" therefore
                // returns almost immediately, the finger comes up after a few milliseconds, and NBGL treats that
                // as a cancelled hold. Re-reading the screen each time makes a stale event cost one extra loop
                // instead of a failed signature.
                let widget = self.wait_for_widget(stream, deadline, NBGL_APPROVE_ANCHOR, screens)?;
                let (x, y) = widget.centre();
                self.api.touch(Touch::Press { x, y })?;
                let gone = self.reported(NBGL_APPROVE_ANCHOR);
                let held = self.wait_until(stream, deadline, "the hold-to-sign to complete", screens, |screen| {
                    !screen.contains(gone.as_str())
                });
                // Lift the finger whatever happened. Leaving it down after a timeout would make every scenario
                // after this one fail in a way that had nothing to do with what they were testing.
                let released = self.api.touch(Touch::Release { x, y });
                held?;
                Ok(released?)
            },
        }
    }

    /// Reject the review the device is showing.
    ///
    /// `screens` is carried through purely so that a cleanup that times out still reports what the review said.
    /// An empty transcript is the worst possible thing to hand somebody whose scenario has just failed twice.
    fn reject(&self, stream: &mut EventStream, deadline: Instant, screens: &[String]) -> Result<(), ReviewError> {
        match self.toolkit() {
            UiToolkit::Bagl => {
                // The reject page is one to the right of the approve page; both buttons choose whichever is shown.
                self.api.press_button(Button::Right)?;
                self.wait_for_widget(stream, deadline, BAGL_REJECT_ANCHOR, screens)?;
                self.api.press_button(Button::Both)?;
                Ok(())
            },
            UiToolkit::Nbgl => {
                let reject = self.wait_for_widget(stream, deadline, NBGL_REJECT_ANCHOR, screens)?;
                let (x, y) = reject.centre();
                self.api.touch(Touch::Tap { x, y })?;
                // Rejecting opens a confirmation dialog, which is a second widget to find by its text.
                let confirm = self.wait_for_widget(stream, deadline, NBGL_CONFIRM_REJECT_ANCHOR, screens)?;
                let (x, y) = confirm.centre();
                self.api.touch(Touch::Tap { x, y })?;
                Ok(())
            },
        }
    }

    /// Block until the device draws a text matching `anchor`, and return it with its geometry.
    ///
    /// This is the text-anchor tap in one function: the caller says which button it wants, not where it is.
    fn wait_for_widget(
        &self,
        stream: &mut EventStream,
        deadline: Instant,
        anchor: &str,
        screens: &[String],
    ) -> Result<ScreenText, ReviewError> {
        let wanted = self.reported(anchor);
        loop {
            let texts = self.api.current_screen()?;
            if let Some(widget) = texts.iter().find(|text| text.text.trim() == wanted) {
                return Ok(widget.clone());
            }
            // A partial match is still useful for the failure message, but never for a tap: tapping the centre of
            // a rectangle that only holds part of the label is how a coordinate table gets reinvented by accident.
            self.wait(stream, deadline, &format!("a {anchor:?} to press"), screens)?;
        }
    }

    /// Block until the screen satisfies `ready`, or fail with everything we saw.
    ///
    /// Always a question about the **screen**, never about the event stream, because the stream is a queue: an
    /// event arriving says something was drawn at some point, not that the thing being waited for has happened.
    /// Re-reading the screen on each wake makes a backlog of stale events cost an extra loop and nothing else.
    fn wait_until<F>(
        &self,
        stream: &mut EventStream,
        deadline: Instant,
        waiting_for: &str,
        screens: &[String],
        ready: F,
    ) -> Result<(), ReviewError>
    where
        F: Fn(&str) -> bool,
    {
        loop {
            if ready(&joined(&self.api.current_screen()?)) {
                return Ok(());
            }
            self.wait(stream, deadline, waiting_for, screens)?;
        }
    }

    /// Block until the device draws something, or fail with everything we saw.
    fn wait(
        &self,
        stream: &mut EventStream,
        deadline: Instant,
        waiting_for: &str,
        screens: &[String],
    ) -> Result<(), ReviewError> {
        match stream.next_event(deadline)? {
            Some(_) => Ok(()),
            None => Err(ReviewError::Timeout {
                waiting_for: waiting_for.to_string(),
                after: self.timeout,
                transcript: Box::new(self.transcript(screens)),
            }),
        }
    }

    fn dress(&self, error: ReviewError, screens: &[String]) -> ReviewError {
        match error {
            ReviewError::Timeout { waiting_for, after, .. } => ReviewError::Timeout {
                waiting_for,
                after,
                transcript: Box::new(self.transcript(screens)),
            },
            other => other,
        }
    }

    /// Collect everything there is to say about what the device did.
    ///
    /// Every lookup in here is allowed to fail without replacing the diagnostic with an error about collecting the
    /// diagnostic: the caller is already reporting a failure, and "the event log could not be read" is strictly
    /// less useful than the half of it that could.
    fn transcript(&self, screens: &[String]) -> Transcript {
        let event_log = self.api.event_log().unwrap_or_default();
        let last_screen = self.api.current_screen().unwrap_or_default();
        let screenshot = self.save_screenshot();
        Transcript {
            screens: screens.to_vec(),
            event_log,
            last_screen,
            screenshot,
        }
    }

    fn save_screenshot(&self) -> Option<PathBuf> {
        let png = self.api.screenshot().ok()?;
        let name = format!(
            "speculos-{}-{}.png",
            self.model,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default()
        );
        let path = env::temp_dir().join(name);
        fs::write(&path, png).ok()?;
        Some(path)
    }
}

impl Approver for SpeculosApprover {
    fn expect_and_approve(&self, expected: &ExpectedReview) -> Result<(), ReviewError> {
        self.run(expected, Outcome::Approve)
    }

    fn expect_and_reject(&self, expected: &ExpectedReview) -> Result<(), ReviewError> {
        self.run(expected, Outcome::Reject)
    }
}

/// An [`Approver`] backed by a human holding a real Ledger.
///
/// This exists so that the same scenarios can be run against hardware, which is the only place some of them mean
/// anything: a simulator cannot tell you that the screen a user actually looks at shows the address the host
/// asked for. The operator reads the expected fields off this prompt, compares them with the device, and answers.
/// If they say the device shows something else, the scenario fails with whatever they were shown.
///
/// `dialoguer` is used for the prompts because `minotari_ledger_wallet_comms` already depends on it for
/// `ledger_demo`; a second prompting crate for two questions would be a dependency nobody needs.
pub struct HumanApprover;

/// What to put in front of the operator, so that they can compare it with the device.
///
/// Built as a string rather than printed directly so that a test can assert every expected value really does reach
/// the operator. A prompt that quietly omitted the receiver would turn the hardware path into a rubber stamp, and
/// nothing else in the harness would notice.
pub fn expectation_banner(expected: &ExpectedReview) -> String {
    let mut banner = String::new();
    banner.push_str("=========================================================================\n");
    banner.push_str("  The device should now be showing a transaction review with exactly\n");
    banner.push_str("  these fields, and no others:\n\n");
    for field in expected.present() {
        banner.push_str(&format!("      {:<12} {}\n", format!("{}:", field.name), field.value));
    }
    for name in expected.absent() {
        banner.push_str(&format!("      {:<12} (must NOT appear at all)\n", format!("{name}:")));
    }
    banner.push_str("\n  Read them off the device, not off this screen. A value that differs by\n");
    banner.push_str("  one character is the whole reason this test exists.\n");
    banner.push_str("=========================================================================");
    banner
}

/// What to tell the operator when a scenario gives up with a review still on the device.
///
/// Pure, so that a test can assert the operator is actually told - see
/// `the_abandon_instructions_tell_the_operator_to_reject_the_review`. There is no way to drive `dialoguer`
/// non-interactively, so the only part of this that can be checked automatically is the part that is a value.
pub fn abandon_instructions(reason: &str) -> String {
    let mut message = String::new();
    message.push_str("\n=========================================================================\n");
    message.push_str("  STOPPING, AND THE DEVICE IS STILL WAITING.\n\n");
    message.push_str(&format!("  Why: {reason}\n\n"));
    message.push_str("  Reject the transaction on the device now.\n\n");
    message.push_str("  This is not tidiness. The instruction is still outstanding on the\n");
    message.push_str("  device and nothing here can cancel it - a Ledger exchange finishes when\n");
    message.push_str("  somebody presses a button and at no other time - so until you do, this\n");
    message.push_str("  process cannot return and tell you what it found.\n");
    message.push_str("=========================================================================");
    message
}

/// The finding when an operator says the device is showing something other than the scenario expected.
///
/// A function rather than an inline `format!` for one reason: this is the single most important sentence this
/// type can produce - a human has just caught the device displaying an address the host did not ask for - and a
/// test asserts that it survives the abandon path with its content intact.
pub fn operator_mismatch_message(expected: &ExpectedReview) -> String {
    format!(
        "The operator says the device is not showing the expected review. Expected: {}",
        expected.summary()
    )
}

impl HumanApprover {
    fn prompt(&self, expected: &ExpectedReview, outcome: Outcome) -> Result<(), ReviewError> {
        println!();
        println!("{}", expectation_banner(expected));

        let matches = match dialoguer::Confirm::new()
            .with_prompt("Does the device show exactly those fields?")
            .default(false)
            .interact()
        {
            Ok(matches) => matches,
            Err(e) => {
                return Err(self.abandon(format!(
                    "Could not ask the operator whether the device matched ({e}). HumanApprover needs a terminal; use \
                     SpeculosApprover for an unattended run."
                )));
            },
        };
        if !matches {
            // The whole reason this type exists, arriving. Everything below is about making sure the operator can
            // actually get this sentence out of the process.
            return Err(self.abandon(operator_mismatch_message(expected)));
        }

        println!("Now {outcome} the transaction on the device itself.");
        let done = match dialoguer::Confirm::new()
            .with_prompt(format!("Have you pressed {outcome} on the device?"))
            .default(false)
            .interact()
        {
            Ok(done) => done,
            Err(e) => return Err(self.abandon(format!("Could not ask the operator to {outcome} ({e})"))),
        };
        if !done {
            return Err(self.abandon(format!("The operator did not {outcome} the review on the device")));
        }
        Ok(())
    }

    /// Give up on a scenario, but not before the device has been answered.
    ///
    /// # Why every error path has to come through here
    ///
    /// Each of the four ways [`Self::prompt`] can fail happens while the review is still on the device and the
    /// instruction that raised it is still parked in an APDU exchange. On the HID transport that exchange has no
    /// bound at all and is documented as having none - see `LedgerTransport::exchange` in
    /// `minotari_ledger_wallet_comms`, which says in as many words that it may block indefinitely and must never
    /// be wrapped in a timeout, because a timeout abandons the reply without cancelling anything on the device.
    /// `while_reviewing` then joins that thread. So returning an error from here without the review being answered
    /// does not report the operator's finding, it *hangs* - and the finding it swallows is the most valuable one
    /// this harness can produce, a human catching the device displaying something the host did not ask for.
    ///
    /// # The returned error is always the original finding
    ///
    /// Not a secondary error about the cleanup. Somebody reading the failure needs to know that the address was
    /// wrong, not that a confirmation prompt was declined afterwards.
    ///
    /// # If the operator will not, or cannot, confirm
    ///
    /// Asking again is the right answer to "not yet" - it costs nothing and each round trip blocks on a human, so
    /// it cannot spin. It is the wrong answer to a prompt that *failed*, which will fail identically for ever, so
    /// that case stops asking and falls back to printing what has to be pressed. The process may then block in the
    /// join until somebody touches the device, and that is not a bug that can be fixed from this side: the device
    /// holds the only cancel button there is. `examples/human_review.rs` refuses to start without a terminal,
    /// which is what keeps that case off the table in the one place this type is actually used.
    fn abandon(&self, reason: String) -> ReviewError {
        println!("{}", abandon_instructions(&reason));
        loop {
            match dialoguer::Confirm::new()
                .with_prompt("Have you rejected the review on the device?")
                .default(false)
                .interact()
            {
                Ok(true) => break,
                Ok(false) => println!(
                    "Still waiting. Reject the transaction on the device - nothing here can do it for you, and this \
                     process cannot return until it is done."
                ),
                Err(e) => {
                    println!(
                        "Cannot ask any more ({e}). Reject the transaction on the device; until you do, this process \
                         is blocked waiting for the device to answer an instruction it was already sent."
                    );
                    break;
                },
            }
        }
        ReviewError::Operator(reason)
    }
}

impl Approver for HumanApprover {
    fn expect_and_approve(&self, expected: &ExpectedReview) -> Result<(), ReviewError> {
        self.prompt(expected, Outcome::Approve)
    }

    fn expect_and_reject(&self, expected: &ExpectedReview) -> Result<(), ReviewError> {
        self.prompt(expected, Outcome::Reject)
    }
}

/// Run a device instruction that puts a review up, and answer the review while it is outstanding.
///
/// The two halves genuinely have to happen at once. The instruction's APDU exchange does not return until somebody
/// answers the review, and answering it means reading the screen and pressing buttons over a different socket, so
/// one of them has to be on another thread. The instruction goes there, because it is the one that blocks; the
/// approver stays on the calling thread, because [`HumanApprover`] needs the terminal.
///
/// Both results come back. A scenario usually wants to assert on both - that the review said the right thing, and
/// that the instruction returned the right thing - and a helper that swallowed either would make half of that
/// impossible.
pub fn while_reviewing<T, F>(
    approver: &dyn Approver,
    expected: &ExpectedReview,
    outcome: Outcome,
    instruction: F,
) -> (T, Result<(), ReviewError>)
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    std::thread::scope(|scope| {
        let handle = scope.spawn(instruction);
        let review = match outcome {
            Outcome::Approve => approver.expect_and_approve(expected),
            Outcome::Reject => approver.expect_and_reject(expected),
        };
        // Propagating the panic keeps a panicking instruction looking like a panicking instruction, rather than
        // turning into a confusing secondary failure about a poisoned transport on the next test.
        let result = match handle.join() {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        (result, review)
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn an_unset_model_is_nanosplus() {
        assert_eq!(model_from(None).unwrap(), DeviceModel::NanoSPlus);
    }

    #[test]
    fn a_model_is_taken_as_given() {
        assert_eq!(model_from(Some("stax")).unwrap(), DeviceModel::Stax);
        assert_eq!(model_from(Some(" stax\n")).unwrap(), DeviceModel::Stax);
    }

    /// Speculos' own name for the model is `nanosp`; this harness uses `cargo ledger build`'s name. A typo must
    /// not silently select the other toolkit and then blame the device for not answering.
    #[test]
    fn an_unrecognised_model_is_an_error() {
        let message = model_from(Some("nanosp")).err().unwrap().to_string();
        assert!(message.contains(SPECULOS_MODEL), "{message}");
        assert!(message.contains("nanosplus"), "{message}");
        assert!(model_from(Some("")).is_err());
    }

    /// The other half of `review::test::the_toolkit_anchors_survive_bagl_reporting`.
    ///
    /// Every anchor is matched against a screen that, on a BAGL model, reached the harness as Speculos'
    /// reconstruction of the pixels rather than as the string the application drew. Two things have to hold, and
    /// they are different things:
    ///
    /// * The anchors that are actually *used* on a BAGL model must come back from that reconstruction unchanged -
    ///   otherwise the reported form and the drawn form disagree and no comparison can succeed either way round.
    /// * The anchors used on NBGL need not, and one of them genuinely does not: `"Sign transaction"` loses its `S`.
    ///   That is harmless only because NBGL text is exact and the comparison goes through `reported`, which is the
    ///   identity there. Compared raw against a BAGL screen it would silently never match - which is exactly the trap
    ///   this pair of tests exists to keep shut.
    #[test]
    fn the_anchors_survive_bagl_reporting() {
        for anchor in [HOME_ANCHOR, BAGL_APPROVE_ANCHOR, BAGL_REJECT_ANCHOR] {
            assert_eq!(
                crate::review::as_bagl_reports(anchor),
                anchor,
                "the anchor {anchor:?} is used on a BAGL model and is not reported verbatim there, so it can no \
                 longer match the screen the device drew"
            );
        }
        for anchor in [
            NBGL_FINISH_ANCHOR,
            NBGL_APPROVE_ANCHOR,
            NBGL_REJECT_ANCHOR,
            NBGL_CONFIRM_REJECT_ANCHOR,
        ] {
            assert_eq!(UiToolkit::Nbgl.as_reported(anchor), anchor);
        }
        // The live counterexample, pinned so that nobody "tidies" an anchor comparison back to a raw `contains`.
        assert_ne!(
            crate::review::as_bagl_reports(NBGL_FINISH_ANCHOR),
            NBGL_FINISH_ANCHOR,
            "if this ever starts holding, the reason the anchors must go through `reported` has changed and the \
             comment above is no longer the truth"
        );
    }

    #[test]
    fn the_swipe_crosses_the_middle_of_the_screen_from_right_to_left() {
        let Touch::Swipe { from, to } = nbgl_swipe(DeviceModel::Stax) else {
            panic!("a swipe is a swipe");
        };
        assert_eq!(from, (320, 336));
        assert_eq!(to, (80, 336));
        assert!(from.0 > to.0, "a next-page swipe goes right to left");
        assert_eq!(from.1, to.1, "and stays on one line");
    }

    #[test]
    fn a_transcript_prints_the_event_log_and_the_last_screen() {
        let transcript = Transcript {
            screens: vec!["Amount12345 uT".to_string()],
            event_log: vec![ScreenText {
                text: "Amount".to_string(),
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            }],
            last_screen: vec![ScreenText {
                text: "Hold to sign".to_string(),
                x: 24,
                y: 496,
                w: 183,
                h: 40,
            }],
            screenshot: None,
        };
        let printed = transcript.to_string();
        assert!(printed.contains("Full event log"), "{printed}");
        assert!(printed.contains("\"Amount\" at (1, 2) 3x4"), "{printed}");
        assert!(printed.contains("Last screen"), "{printed}");
        assert!(printed.contains("Hold to sign"), "{printed}");
    }

    /// A timeout with no diagnostics is the failure mode most likely to make people abandon a suite, so the
    /// message has to carry the log and the screen rather than a bare "timed out".
    #[test]
    fn a_timeout_prints_everything_the_device_drew() {
        let error = ReviewError::Timeout {
            waiting_for: "the review to appear".to_string(),
            after: Duration::from_secs(2),
            transcript: Box::new(Transcript {
                screens: vec!["Review Transaction".to_string()],
                event_log: vec![ScreenText {
                    text: "MinoTari".to_string(),
                    x: 40,
                    y: 28,
                    w: 10,
                    h: 12,
                }],
                last_screen: vec![ScreenText {
                    text: "Wallet".to_string(),
                    x: 48,
                    y: 42,
                    w: 10,
                    h: 12,
                }],
                screenshot: Some(PathBuf::from("/tmp/shot.png")),
            }),
        };
        let printed = error.to_string();
        assert!(printed.contains("the review to appear"), "{printed}");
        assert!(printed.contains("Full event log"), "{printed}");
        assert!(printed.contains("MinoTari"), "{printed}");
        assert!(printed.contains("Last screen"), "{printed}");
        assert!(printed.contains("/tmp/shot.png"), "{printed}");
    }

    /// The operator is the assertion oracle on hardware, so everything they have to check must be in front of
    /// them - including the fields that must *not* be there, which are the easiest thing for a prompt to forget.
    #[test]
    fn the_human_prompt_shows_every_value_the_operator_has_to_compare() {
        let receiver = "232F5WN4VC6zJL3f2YbGw8w8kFNmL5XvN6nKkUrwWuArLd5k4P9oCBsafEyQXkxCGSa89o74R18Aw5reCSKwVFtgLg5";

        let banner = expectation_banner(&ExpectedReview::one_sided_metadata_signature(12_345, receiver, 0));
        assert!(banner.contains("Amount:"), "{banner}");
        assert!(banner.contains("12345 uT"), "{banner}");
        assert!(banner.contains("Receiver:"), "{banner}");
        assert!(banner.contains(receiver), "{banner}");
        assert!(banner.contains("Payment ID:"), "{banner}");
        assert!(banner.contains("must NOT appear"), "{banner}");

        let banner = expectation_banner(&ExpectedReview::one_sided_metadata_signature(2_500_000, receiver, 16));
        assert!(banner.contains("2.50 T"), "{banner}");
        assert!(banner.contains("16 bytes"), "{banner}");
        assert!(!banner.contains("must NOT appear"), "{banner}");
    }

    /// Every `HumanApprover` failure leaves a review outstanding on a device that nothing here can cancel, so the
    /// operator has to be told to reject it - and told *why*, because the reason is the finding the whole exercise
    /// exists to produce.
    ///
    /// `dialoguer` cannot be driven without a terminal, so the assertion is on the message, which is a value for
    /// exactly this reason. The mismatch path is the one checked in full: it is the operator-as-oracle catching
    /// the device showing an address the host never asked for, and it must survive the cleanup with its content
    /// intact rather than being replaced by a complaint about a confirmation prompt.
    #[test]
    fn the_abandon_instructions_tell_the_operator_to_reject_the_review() {
        let receiver = "232F5WN4VC6zJL3f2YbGw8w8kFNmL5XvN6nKkUrwWuArLd5k4P9oCBsafEyQXkxCGSa89o74R18Aw5reCSKwVFtgLg5";
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, receiver, 0);

        let finding = operator_mismatch_message(&expected);
        assert!(finding.contains(receiver), "{finding}");
        assert!(finding.contains("12345 uT"), "{finding}");

        let instructions = abandon_instructions(&finding);
        assert!(
            instructions.contains("Reject the transaction on the device"),
            "the operator must be told what to press, got:\n{instructions}"
        );
        assert!(
            instructions.contains("cannot return"),
            "and why they have to, got:\n{instructions}"
        );
        // The finding survives into the cleanup message rather than being replaced by it.
        assert!(instructions.contains(receiver), "{instructions}");
        assert!(
            instructions.contains("not showing the expected review"),
            "{instructions}"
        );

        // And the error a caller finally receives is the finding, not a report about the cleanup.
        assert_eq!(
            ReviewError::Operator(finding.clone()).to_string(),
            finding,
            "the returned error must still be the operator's finding"
        );
    }

    #[test]
    fn a_not_at_home_error_says_what_was_on_the_screen() {
        let printed = ReviewError::NotAtHome {
            screen: "Reject transaction?".to_string(),
        }
        .to_string();
        assert!(printed.contains("Reject transaction?"), "{printed}");
    }
}
