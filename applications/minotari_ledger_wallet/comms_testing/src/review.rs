// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! What the device is supposed to be showing, and whether it is.
//!
//! # One instruction shows a review screen
//!
//! `GetOneSidedMetadataSignature` is the only handler in the device application that puts anything in front of a
//! human - see `wallet/src/handlers/get_one_sided_metadata_signature.rs`. Everything else answers without asking.
//! So this module is not a UI framework; it is a model of one screen, and it is sized accordingly.
//!
//! That screen carries three fields:
//!
//! | Field        | Value                                                          |
//! |--------------|----------------------------------------------------------------|
//! | `Amount`     | the requested value, formatted by the device's own `Minotari`   |
//! | `Receiver`   | the requested receiver, as a base58 Tari dual address           |
//! | `Payment ID` | `"{N} bytes"` - **present only if the payment ID is non-empty** |
//!
//! # Text, not screenshots
//!
//! The assertion is on the **text of the fields**. That is the whole value of the exercise: a wrong key bricks a
//! wallet, but a wrong *displayed* address steals funds, because the screen is the only thing a user can trust
//! when the host cannot be. An address that the device shows and the host did not ask for is the attack a hardware
//! wallet exists to stop, and it is invisible to every other test in this repository.
//!
//! Screenshot baselines were considered and rejected. They fail on pixel changes that harm nobody - a font bump, a
//! one-pixel layout shift - and the fix for a red baseline is to re-bless it, which is a single keystroke that
//! also blesses any real change hiding behind the cosmetic one. Both failure modes train reviewers to stop reading
//! the diff. A text assertion fails only when the text changed, and the text is the thing being defended.
//!
//! # BAGL text is lossy, and pretending otherwise would be worse
//!
//! Speculos reports what a device drew in two very different ways:
//!
//! * **NBGL** models (`stax`, `flex`) hand it the string, and it reports the string. Exact.
//! * **BAGL** models (`nanosplus`, `nanox`) draw one glyph bitmap at a time. Speculos reconstructs the text by matching
//!   each bitmap against its own built-in copy of the font tables - see `speculos/mcu/ocr.py`. Where the application's
//!   font table and Speculos' copy disagree, the reconstruction is wrong or absent, **while the pixels on screen are
//!   perfectly correct**.
//!
//! Measured against `ledger_device_sdk 1.35.0` and the Speculos image pinned in `scripts/ledger_speculos.sh`, by
//! comparing every one of the SDK's 96 `OPEN_SANS` glyph bitmaps against `speculos.mcu.ocr.find_char_from_bitmap`,
//! the disagreement is exactly two characters and no others:
//!
//! * `'S'` in the regular (non-bold) face produces **no character at all** - the SDK's bitmap is `0027080606410e0000`
//!   and Speculos' nearest is `8027080606411e0000`, so no font matches and the glyph is dropped from the event.
//! * `'I'` is reported as `'l'` in both faces.
//!
//! [`UiToolkit::Bagl`] therefore compares against [`as_bagl_reports`] - the expected text with those two
//! substitutions applied - and this is written down here rather than hidden in an `assert` so that whoever bumps
//! the SDK or the Speculos digest can re-derive it. To do so: dump the SDK's `OPEN_SANS_REGULAR_11PX_CHARS` and
//! `OPEN_SANS_EXTRABOLD_11PX_CHARS` from `src/ui/fonts/opensans.rs` and feed each bitmap to
//! `speculos.mcu.ocr.OCR.find_char_from_bitmap` inside the Speculos container.
//!
//! What this costs is worth stating plainly, and the two substitutions cost very different things.
//!
//! **`'I'` costs nothing, and the reason is the base58 alphabet.** Both encoders involved - `TariAddress::
//! to_base58` in `base_layer/common_types/src/tari_address/mod.rs` and `tari_dual_address_display` in
//! `applications/minotari_ledger_wallet/common/src/utils.rs` - use `bs58`'s default Bitcoin alphabet, which
//! deliberately omits `0`, `O`, `I` and `l` because they are easy to confuse by eye. So no address, and no
//! `"{n} bytes"` payment ID length, can contain either side of the `I`/`l` conflation. The only `I` anywhere in
//! this review is the one in the literal field name `"Payment ID"`, where turning it into `"Payment lD"` on both
//! sides of the comparison changes nothing. That is not luck holding, it is a property of the encoding, and it is
//! why this substitution is safe rather than merely untriggered.
//!
//! **`'S'` does cost something.** It is in the base58 alphabet, so on BAGL models a device that dropped an `'S'`
//! from the address it displayed would not be caught - that is a real, if narrow, hole in the strongest assertion
//! here. Every other character is compared exactly, and `stax` reports text losslessly and so compares all of
//! them, which is one more reason the suite runs on both models rather than one.

use std::fmt;

/// Which Ledger model the simulator is running.
///
/// Only the two the harness is run against are modelled as separate arms of the UI decision; `nanox` and `flex`
/// are recognised because `scripts/ledger_speculos.sh` accepts them and a typo should fail loudly rather than
/// silently choose the wrong toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceModel {
    NanoSPlus,
    NanoX,
    Stax,
    Flex,
}

impl DeviceModel {
    /// Every model this harness knows, in the order `scripts/ledger_speculos.sh` lists them.
    pub const ALL: [DeviceModel; 4] = [
        DeviceModel::NanoSPlus,
        DeviceModel::NanoX,
        DeviceModel::Stax,
        DeviceModel::Flex,
    ];

    /// The name used on the command line and in `SPECULOS_MODEL`, which is `cargo ledger build`'s name for the
    /// model rather than Speculos' own (`nanosplus`, not `nanosp`).
    pub const fn name(self) -> &'static str {
        match self {
            DeviceModel::NanoSPlus => "nanosplus",
            DeviceModel::NanoX => "nanox",
            DeviceModel::Stax => "stax",
            DeviceModel::Flex => "flex",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        DeviceModel::ALL.into_iter().find(|model| model.name() == name)
    }

    /// Which UI toolkit the device application was compiled against for this model.
    ///
    /// This mirrors the `#[cfg(any(target_os = "stax", target_os = "flex"))]` split in
    /// `handlers/get_one_sided_metadata_signature.rs` exactly. If a model is added there, it is added here, and the
    /// two must not be allowed to drift - a mismatch means the harness drives buttons at a touchscreen.
    pub const fn toolkit(self) -> UiToolkit {
        match self {
            DeviceModel::NanoSPlus | DeviceModel::NanoX => UiToolkit::Bagl,
            DeviceModel::Stax | DeviceModel::Flex => UiToolkit::Nbgl,
        }
    }
}

impl fmt::Display for DeviceModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Which of the SDK's two user interfaces the device is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiToolkit {
    /// Buttons and a small monochrome screen. Text reaches Speculos as reconstructed pixels; see the module docs.
    Bagl,
    /// A touchscreen. Text reaches Speculos as text, with the rectangle it was drawn in.
    Nbgl,
}

impl UiToolkit {
    /// The text Speculos will report, given the text the device was asked to draw.
    ///
    /// Identity for NBGL. For BAGL it is the two measured substitutions described in the module docs, and nothing
    /// else - a general "normalise the string" step would quietly absorb real differences, which is the opposite
    /// of what this assertion is for.
    pub fn as_reported(self, text: &str) -> String {
        match self {
            UiToolkit::Bagl => as_bagl_reports(text),
            UiToolkit::Nbgl => text.to_string(),
        }
    }
}

/// The text Speculos' BAGL pixel reconstruction will report, given the text the device drew.
///
/// `'S'` vanishes and `'I'` becomes `'l'`. See the module docs for how that was measured and why it is not simply
/// a bug to be worked around: the pixels are right, so nothing in the device is wrong, and there is nothing in
/// this repository to fix.
///
/// The `'I'` substitution weakens nothing, because the base58 alphabet both address encoders use excludes `I` and
/// `l` outright; `'S'` is in the alphabet and does leave a narrow hole on BAGL models. The module docs set out
/// both in full.
pub fn as_bagl_reports(text: &str) -> String {
    text.chars()
        .filter(|c| *c != 'S')
        .map(|c| if c == 'I' { 'l' } else { c })
        .collect()
}

/// One row of the review screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedField {
    pub name: String,
    pub value: String,
}

/// What the device must be showing before a scenario is allowed to approve it.
///
/// A scenario that cannot say what it expects on screen cannot be run by [`crate::approver::HumanApprover`], and
/// so cannot be run on real hardware at all. That is deliberate. The constraint is what keeps one scenario library
/// usable by both the simulator and a human holding a device, instead of two libraries that drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedReview {
    present: Vec<ExpectedField>,
    absent: Vec<String>,
}

/// The `Amount` field's value, formatted the way the device formats it.
///
/// This is a copy of `Minotari::to_string` in `handlers/get_one_sided_metadata_signature.rs`, and it is a copy on
/// purpose: the device's version is `no_std` code inside a binary that cannot be linked from here, and an
/// assertion that called the device's own formatter would agree with the device by construction and therefore
/// assert nothing. Two independent statements of the same rule is the point.
///
/// Note that this is **not** a plain decimal rendering of the `u64`: below one million the device shows
/// microTari, and at or above it shows Tari to two decimal places. A test that assumed the simpler thing would
/// fail against a perfectly correct device.
pub fn minotari_amount(value: u64) -> String {
    if value < 1_000_000 {
        format!("{value} uT")
    } else {
        #[allow(clippy::cast_precision_loss)]
        let tari = value as f64 / 1_000_000.0;
        format!("{tari:.2} T")
    }
}

/// The field name the device uses for the amount.
pub const AMOUNT_FIELD: &str = "Amount";
/// The field name the device uses for the receiver address.
pub const RECEIVER_FIELD: &str = "Receiver";
/// The field name the device uses for the payment ID, which it omits entirely when there is no payment ID.
pub const PAYMENT_ID_FIELD: &str = "Payment ID";

impl ExpectedReview {
    /// The review `GetOneSidedMetadataSignature` puts up for `value` going to `receiver`.
    ///
    /// `receiver` is the base58 the *device* will render, which is `TariAddress::to_base58()` - the two
    /// encodings are the same three-part `bs58` construction, one in
    /// `base_layer/common_types/src/tari_address/mod.rs` and one in
    /// `applications/minotari_ledger_wallet/common/src/utils.rs`.
    ///
    /// `payment_id_len` is the number of payment ID bytes carried by the address. Zero means the device shows no
    /// `Payment ID` row at all, and this asserts its **absence** - which is the half of the requirement that a
    /// "does it contain what I expect" check would never catch.
    pub fn one_sided_metadata_signature(value: u64, receiver: &str, payment_id_len: usize) -> Self {
        let mut present = vec![
            ExpectedField {
                name: AMOUNT_FIELD.to_string(),
                value: minotari_amount(value),
            },
            ExpectedField {
                name: RECEIVER_FIELD.to_string(),
                value: receiver.to_string(),
            },
        ];
        let mut absent = Vec::new();
        if payment_id_len > 0 {
            present.push(ExpectedField {
                name: PAYMENT_ID_FIELD.to_string(),
                value: format!("{payment_id_len} bytes"),
            });
        } else {
            absent.push(PAYMENT_ID_FIELD.to_string());
        }
        Self { present, absent }
    }

    /// The fields that must appear, in the order the device draws them.
    pub fn present(&self) -> &[ExpectedField] {
        &self.present
    }

    /// The field names that must **not** appear anywhere in the review.
    pub fn absent(&self) -> &[String] {
        &self.absent
    }

    /// Replace one field's value, for a test that needs a deliberately wrong expectation.
    ///
    /// Acceptance criterion 3 - "a wrong expected address makes the test fail" - needs a way to build a wrong
    /// expectation, and an assertion nobody has ever seen fail is not an assertion. This exists so that the
    /// failure path is exercised by a test rather than asserted in a comment.
    ///
    /// # Panics
    ///
    /// Panics if there is no such field, because a test that meant to corrupt `Receiver` and silently corrupted
    /// nothing would pass for the wrong reason.
    #[must_use]
    pub fn with_field_value(mut self, name: &str, value: &str) -> Self {
        let names: Vec<&str> = self.present.iter().map(|field| field.name.as_str()).collect();
        let Some(field) = self.present.iter().position(|field| field.name == name) else {
            panic!("there is no '{name}' field to change; the review has {names:?}");
        };
        if let Some(field) = self.present.get_mut(field) {
            field.value = value.to_string();
        }
        self
    }

    /// A one line summary, for a prompt or a failure message.
    pub fn summary(&self) -> String {
        self.present
            .iter()
            .map(|field| format!("{}: {}", field.name, field.value))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The text the device should have drawn, once the toolkit's own furniture is taken away.
    fn as_expected_text(&self, toolkit: UiToolkit) -> String {
        self.present
            .iter()
            .map(|field| {
                format!(
                    "{}{}",
                    toolkit.as_reported(&field.name),
                    toolkit.as_reported(&field.value)
                )
            })
            .collect()
    }

    /// Check what the device actually showed.
    ///
    /// `screens` is the text of each screen of the review, in the order they appeared; see
    /// [`crate::approver::SpeculosApprover`] for how they are collected.
    ///
    /// # This is an equality, not a containment
    ///
    /// The screens are stripped of the toolkit's own furniture - the titles, the footers, the page counters; see
    /// [`strip_furniture`] - and what is left must be **exactly** the expected fields, name then value, in order.
    ///
    /// Containment was tried first and is not good enough, for a reason worth keeping written down. `review
    /// contains "Receiver" + address` is satisfied by a device that displays the expected address *followed by
    /// something else*, because the expected text is a prefix of what is on screen. An address with extra
    /// characters on the end is a different address, and it is precisely the kind of thing a hardware wallet's
    /// screen exists to reveal. Equality also gets the absence checks for free: a `Payment ID` row the host never
    /// asked for cannot survive an exact comparison, whether or not anybody remembered to assert it absent.
    ///
    /// Every mismatch is reported, not just the first. A run that says "the receiver is wrong" and stops leaves
    /// you wondering whether the amount was wrong too, and a review is cheap to read in full.
    pub fn check(&self, toolkit: UiToolkit, screens: &[String]) -> Result<(), Vec<String>> {
        let names: Vec<String> = self
            .present
            .iter()
            .map(|field| toolkit.as_reported(&field.name))
            .chain(self.absent.iter().map(|name| toolkit.as_reported(name)))
            .collect();
        let review = review_text(toolkit, screens, &names);
        let expected = self.as_expected_text(toolkit);

        if review == expected {
            return Ok(());
        }

        let mut problems = Vec::new();
        // A review with no text at all would make every "must not appear" check below pass, so the vacuous case is
        // called out explicitly rather than reported as a long diff against an empty string. This is the same
        // argument as `assert_table_is_populated` in the device tests: an assertion that cannot fail is not an
        // assertion.
        if review.is_empty() {
            problems.push("the device showed no text at all, so nothing below was actually checked".to_string());
            return Err(problems);
        }

        // The equality above is the assertion; everything from here down exists to say *which* field went wrong,
        // because a diff of two four-hundred character strings is not a diagnosis.
        for field in &self.present {
            let name = toolkit.as_reported(&field.name);
            let value = toolkit.as_reported(&field.value);
            if !review.contains(&name) {
                problems.push(format!("'{}' does not appear on the review at all", field.name));
            } else if !review.contains(&format!("{name}{value}")) {
                problems.push(format!(
                    "'{}' is not followed by the expected value.\n     expected: {}\n     on screen: {}",
                    field.name,
                    field.value,
                    shown_after(&review, &name)
                ));
            }
        }

        for name in &self.absent {
            let reported = toolkit.as_reported(name);
            if review.contains(&reported) {
                problems.push(format!(
                    "'{name}' must not appear on this review, but it does: {}",
                    shown_after(&review, &reported)
                ));
            }
        }

        if problems.is_empty() {
            // Every field is there and correct, and the review still is not what was expected: the device showed
            // something extra, or showed it somewhere else. Say so rather than reporting nothing and failing.
            problems.push(
                "every expected field is present and correct, but the review carries text beyond them - the device \
                 showed something that was not asked for"
                    .to_string(),
            );
        }
        problems.push(format!("expected the review to read: {expected}"));
        problems.push(format!("the review read:             {review}"));
        Err(problems)
    }
}

/// How much of a value to quote back in a mismatch message.
///
/// Long enough to tell two base58 addresses apart at a glance, short enough that a failure message stays readable.
const QUOTED_VALUE_LENGTH: usize = 120;

fn shown_after(review: &str, name: &str) -> String {
    match review.find(name) {
        Some(index) => {
            let start = index.saturating_add(name.len());
            let rest = review.get(start..).unwrap_or_default();
            let end = rest
                .char_indices()
                .nth(QUOTED_VALUE_LENGTH)
                .map_or(rest.len(), |(index, _)| index);
            rest.get(..end).unwrap_or_default().to_string()
        },
        None => String::new(),
    }
}

/// Flatten the review's screens into the one string the assertions run against.
///
/// Three things happen here, and all of them are needed before a wrapped value can be compared with the value that
/// was asked for.
///
/// **Screens are concatenated with nothing between them.** Both toolkits split a long value across screens, so any
/// separator would break the address in a place the device did not.
///
/// **The toolkit's furniture is removed**, so that what remains is only what the *application* asked to be drawn.
/// See [`strip_furniture`].
///
/// **BAGL's per-page field headers are removed.** When a value does not fit on one page, BAGL repeats the field
/// name with a page counter - `Receiver (1/2)`, `Receiver (2/2)` - between the chunks of the value. Left in, the
/// address would read `Receiver<first half>Receiver (2/2)<second half>` and no contiguous comparison could
/// succeed. The counter is dropped from the first page and the whole header from every later one, so that the
/// value ends up contiguous and still preceded by exactly one field name.
fn review_text(toolkit: UiToolkit, screens: &[String], names: &[String]) -> String {
    screens
        .iter()
        .map(|screen| strip_page_headers(strip_furniture(toolkit, screen.trim()), names))
        .collect::<String>()
}

/// The text the SDK draws around a review, which belongs to the toolkit rather than to the application.
///
/// **Measured against `ledger_device_sdk = "=1.35.0"`** - pinned in
/// `applications/minotari_ledger_wallet/wallet/Cargo.toml` - and the Speculos image digest pinned in
/// `scripts/ledger_speculos.sh`. The strings come from two places and both are worth knowing:
///
/// * the **application's**, from the handler's own arguments - `MultiFieldReview::new(.., &["Review ", "Transaction"],
///   .., "Approve", .., "Reject", ..)` on BAGL, and `.titles("Review transaction\nto send", "", "Sign transaction\nto
///   send")` on NBGL;
/// * the **SDK's**, which the application does not choose - `Swipe to review`, `Hold to sign`, and the `i of n` page
///   counter in the NBGL footer.
///
/// An SDK bump that changes any of them fails the comparison loudly, with the whole review printed next to what
/// was expected, which is the right way for it to be noticed.
const BAGL_FURNITURE: &[&str] = &["Review Transaction", "Approve", "Reject"];
const NBGL_FURNITURE: &[&str] = &[
    "Review transaction",
    "Sign transaction",
    "Swipe to review",
    "Hold to sign",
    "to send",
    "Reject",
];

/// Remove the toolkit's furniture from one screen.
///
/// Only from the **ends** of the screen, never from the middle, and that restriction is the whole reason this is
/// safe. `Reject` is six characters every one of which is in the base58 alphabet, so an address containing it
/// somewhere in the middle is not impossible - it is merely unlikely - and a search-and-replace over the whole
/// screen would then quietly delete six characters of the value being asserted. Every piece of furniture is drawn
/// either before or after the fields, so stripping only prefixes and suffixes loses nothing and cannot reach into
/// a value.
///
/// The furniture is compared **as the toolkit reports it**, not as the application wrote it. Today every entry in
/// both lists survives [`as_bagl_reports`] untouched, so this is a no-op - but that is a property of the strings
/// Ledger happens to have chosen, not of anything here, and the day one of them contains an `S` (a "Settings", a
/// "Sign") the raw comparison would silently stop matching and the furniture would be asserted as though the
/// application had drawn it. `the_toolkit_anchors_survive_bagl_reporting` checks the assumption rather than
/// leaving it to be discovered.
fn strip_furniture(toolkit: UiToolkit, screen: &str) -> &str {
    let furniture: Vec<String> = match toolkit {
        UiToolkit::Bagl => BAGL_FURNITURE,
        UiToolkit::Nbgl => NBGL_FURNITURE,
    }
    .iter()
    .map(|item| toolkit.as_reported(item))
    .collect();
    let mut rest = screen;
    loop {
        let before = rest;
        rest = rest.trim();
        // The NBGL footer is `Reject` then `i of n`, so the counter has to come off before the word above it.
        rest = strip_page_counter_suffix(rest);
        for item in &furniture {
            rest = rest.strip_prefix(item.as_str()).unwrap_or(rest);
            rest = rest.strip_suffix(item.as_str()).unwrap_or(rest);
        }
        if rest == before {
            return rest;
        }
    }
}

/// Every piece of furniture, so that the test below can check the assumption `strip_furniture` rests on.
#[cfg(test)]
fn furniture(toolkit: UiToolkit) -> &'static [&'static str] {
    match toolkit {
        UiToolkit::Bagl => BAGL_FURNITURE,
        UiToolkit::Nbgl => NBGL_FURNITURE,
    }
}

/// Strip a trailing NBGL `i of n` page counter.
fn strip_page_counter_suffix(text: &str) -> &str {
    let Some(rest) = strip_ascii_digits_suffix(text) else {
        return text;
    };
    let Some(rest) = rest.strip_suffix(" of ") else {
        return text;
    };
    strip_ascii_digits_suffix(rest).unwrap_or(text)
}

/// Strip a non-empty run of trailing ASCII digits, or `None` if there is not one.
fn strip_ascii_digits_suffix(text: &str) -> Option<&str> {
    let end = text.trim_end_matches(|c: char| c.is_ascii_digit());
    (end.len() < text.len()).then_some(end)
}

/// Remove BAGL's `name (i/n)` page headers, keeping a single bare `name` for the first page.
///
/// This is a no-op on NBGL, which does not paginate within a field this way, and on any BAGL field short enough to
/// fit on one page - the SDK only emits a counter when there is more than one page.
fn strip_page_headers(screen: &str, names: &[String]) -> String {
    let mut output = String::with_capacity(screen.len());
    let mut rest = screen;
    'outer: while !rest.is_empty() {
        for name in names {
            if name.is_empty() {
                continue;
            }
            let Some(after_name) = rest.strip_prefix(name.as_str()) else {
                continue;
            };
            let Some((page, after_counter)) = parse_page_counter(after_name) else {
                continue;
            };
            // Page one keeps the field name so that the value is still introduced by it; later pages are pure
            // continuation and contribute only their share of the value.
            if page <= 1 {
                output.push_str(name);
            }
            rest = after_counter;
            continue 'outer;
        }
        // Not a header here; copy one character and try again at the next position.
        let mut characters = rest.chars();
        if let Some(character) = characters.next() {
            output.push(character);
        }
        rest = characters.as_str();
    }
    output
}

/// Parse a ` (i/n)` page counter off the front of `text`, returning `i` and the rest.
fn parse_page_counter(text: &str) -> Option<(usize, &str)> {
    let rest = text.strip_prefix(" (")?;
    let (page, rest) = take_digits(rest)?;
    let rest = rest.strip_prefix('/')?;
    let (_count, rest) = take_digits(rest)?;
    let rest = rest.strip_prefix(')')?;
    Some((page, rest))
}

fn take_digits(text: &str) -> Option<(usize, &str)> {
    let end = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let digits = text.get(..end)?;
    if digits.is_empty() {
        return None;
    }
    Some((digits.parse().ok()?, text.get(end..)?))
}

#[cfg(test)]
mod test {
    use super::*;

    /// The captured `nanosplus` screens for a 91 character receiver, exactly as Speculos reported them - note the
    /// two missing `'S'` characters, which is the lossy reconstruction described in the module docs.
    fn bagl_screens(receiver_as_reported: &str) -> Vec<String> {
        let (first, second) = receiver_as_reported.split_at(50);
        vec![
            "Review Transaction".to_string(),
            "Amount12345 uT".to_string(),
            format!("Receiver (1/2){first}"),
            format!("Receiver (2/2){second}"),
            "Approve".to_string(),
        ]
    }

    /// The captured `stax` screens for the same review.
    fn nbgl_screens(receiver: &str) -> Vec<String> {
        vec![
            "Review transactionto sendSwipe to reviewReject1 of 3".to_string(),
            format!("Amount12345 uTReceiver{receiver}Reject2 of 3"),
            "Sign transactionto sendReject3 of 3Hold to sign".to_string(),
        ]
    }

    const RECEIVER: &str =
        "232F5WN4VC6zJL3f2YbGw8w8kFNmL5XvN6nKkUrwWuArLd5k4P9oCBsafEyQXkxCGSa89o74R18Aw5reCSKwVFtgLg5";

    #[test]
    fn the_amount_is_formatted_the_way_the_device_formats_it() {
        // Not a plain decimal: below a million the device shows microTari.
        assert_eq!(minotari_amount(0), "0 uT");
        assert_eq!(minotari_amount(12_345), "12345 uT");
        assert_eq!(minotari_amount(999_999), "999999 uT");
        // And at a million it switches units and rounds to two places.
        assert_eq!(minotari_amount(1_000_000), "1.00 T");
        assert_eq!(minotari_amount(1_234_567), "1.23 T");
    }

    #[test]
    fn the_bagl_substitutions_are_the_two_that_were_measured_and_no_others() {
        assert_eq!(as_bagl_reports("S"), "");
        assert_eq!(as_bagl_reports("I"), "l");
        assert_eq!(as_bagl_reports("Payment ID"), "Payment lD");
        // Lower case is untouched - it is capital S that has no matching bitmap, not the letter.
        assert_eq!(as_bagl_reports("s"), "s");
        assert_eq!(as_bagl_reports("Receiver"), "Receiver");
        assert_eq!(as_bagl_reports("12345 uT"), "12345 uT");
    }

    #[test]
    fn a_correct_nbgl_review_passes() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, RECEIVER, 0);
        expected.check(UiToolkit::Nbgl, &nbgl_screens(RECEIVER)).unwrap();
    }

    #[test]
    fn a_correct_bagl_review_passes() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, RECEIVER, 0);
        let screens = bagl_screens(&as_bagl_reports(RECEIVER));
        expected.check(UiToolkit::Bagl, &screens).unwrap();
    }

    /// Acceptance criterion 3, without a device: a wrong expected address must fail.
    ///
    /// The device tests cover the same thing against a live simulator, but this one runs on every `cargo test`, so
    /// a change that made the comparison vacuous - a normalisation that strips too much, a `contains` on the
    /// wrong string - is caught before anybody starts Docker.
    #[test]
    fn a_wrong_expected_receiver_fails_on_both_toolkits() {
        let mut wrong = RECEIVER.to_string();
        wrong.replace_range(10..11, "Z");
        assert_ne!(wrong, RECEIVER);

        let expected = ExpectedReview::one_sided_metadata_signature(12_345, &wrong, 0);

        let problems = expected
            .check(UiToolkit::Nbgl, &nbgl_screens(RECEIVER))
            .expect_err("a wrong receiver must not pass on NBGL");
        assert!(
            problems.iter().any(|p| p.contains("'Receiver'")),
            "the failure should name the field, got: {problems:?}"
        );

        let problems = expected
            .check(UiToolkit::Bagl, &bagl_screens(&as_bagl_reports(RECEIVER)))
            .expect_err("a wrong receiver must not pass on BAGL");
        assert!(
            problems.iter().any(|p| p.contains("'Receiver'")),
            "the failure should name the field, got: {problems:?}"
        );
    }

    #[test]
    fn a_wrong_expected_amount_fails() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_346, RECEIVER, 0);
        let problems = expected
            .check(UiToolkit::Nbgl, &nbgl_screens(RECEIVER))
            .expect_err("a wrong amount must not pass");
        assert!(problems.iter().any(|p| p.contains("'Amount'")), "{problems:?}");
    }

    /// A truncated address must fail even though the expected value is a prefix of what is on screen, because
    /// `contains` on the value alone would happily match the first half of a longer address.
    #[test]
    fn a_truncated_expected_receiver_fails() {
        let truncated = RECEIVER.get(..40).unwrap();
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, truncated, 0);
        let problems = expected
            .check(UiToolkit::Nbgl, &nbgl_screens(RECEIVER))
            .expect_err("a prefix of the address on screen is not the address on screen");
        assert!(
            problems.iter().any(|p| p.contains("beyond them")),
            "the failure should say the device showed more than was expected, got: {problems:?}"
        );
        assert!(
            expected
                .check(UiToolkit::Bagl, &bagl_screens(&as_bagl_reports(RECEIVER)))
                .is_err()
        );
    }

    /// The furniture the toolkits draw around a review is not the application's text and must not be asserted as
    /// though it were - but it also must not be stripped from anywhere except the ends of a screen, because
    /// `Reject` is six perfectly ordinary base58 characters.
    #[test]
    fn toolkit_furniture_is_stripped_from_the_ends_of_a_screen_only() {
        assert_eq!(
            strip_furniture(UiToolkit::Nbgl, "Review transactionto sendSwipe to reviewReject1 of 3"),
            ""
        );
        assert_eq!(
            strip_furniture(UiToolkit::Nbgl, "Sign transactionto sendReject3 of 3Hold to sign"),
            ""
        );
        assert_eq!(
            strip_furniture(UiToolkit::Nbgl, "Amount12345 uTReceiverabcReject2 of 3"),
            "Amount12345 uTReceiverabc"
        );
        assert_eq!(strip_furniture(UiToolkit::Bagl, "Review Transaction"), "");
        assert_eq!(strip_furniture(UiToolkit::Bagl, "Approve"), "");
        assert_eq!(strip_furniture(UiToolkit::Bagl, "Amount12345 uT"), "Amount12345 uT");

        // A value with furniture-shaped text buried in the middle of it keeps every character.
        assert_eq!(
            strip_furniture(UiToolkit::Nbgl, "ReceiverabcRejectdefReject2 of 3"),
            "ReceiverabcRejectdef"
        );
        // And a value that happens to end in digits is not mistaken for a page counter.
        assert_eq!(strip_furniture(UiToolkit::Bagl, "Receiver4wBqpZM5"), "Receiver4wBqpZM5");
    }

    /// Every string this harness compares against a BAGL screen must mean the same thing after Speculos'
    /// reconstruction has had it - otherwise the comparison silently stops matching and the harness starts
    /// asserting furniture as though it were the application's own text.
    ///
    /// It holds today by luck rather than by design: none of these strings contains an `S` or an `I`. This test is
    /// what turns the luck into a checked precondition, so that an SDK or application string that breaks it fails
    /// here with the string named, rather than three layers down as a mystery timeout.
    #[test]
    fn the_toolkit_anchors_survive_bagl_reporting() {
        for item in furniture(UiToolkit::Bagl) {
            assert_eq!(
                &as_bagl_reports(item),
                item,
                "the BAGL furniture string {item:?} is not reported verbatim by Speculos; strip_furniture compares it \
                 against a reconstructed screen and would no longer match"
            );
        }
        // NBGL text is exact, so its furniture has nothing to survive - asserted anyway, because the same list is
        // reached through `as_reported` and a change of toolkit mapping should not pass unnoticed.
        for item in furniture(UiToolkit::Nbgl) {
            assert_eq!(&UiToolkit::Nbgl.as_reported(item), item);
        }
    }

    #[test]
    fn a_page_counter_suffix_is_only_stripped_when_it_is_one() {
        assert_eq!(strip_page_counter_suffix("Reject12 of 34"), "Reject");
        assert_eq!(strip_page_counter_suffix("abc"), "abc");
        assert_eq!(strip_page_counter_suffix("abc5"), "abc5");
        assert_eq!(strip_page_counter_suffix("abc of 5"), "abc of 5");
        assert_eq!(strip_page_counter_suffix("1 of "), "1 of ");
    }

    #[test]
    fn a_missing_payment_id_row_is_asserted_absent() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, RECEIVER, 0);
        assert_eq!(expected.absent(), ["Payment ID"]);

        // A device that grew a Payment ID row it was not asked for must fail, on both toolkits - and on BAGL the
        // label arrives as "Payment lD", so a naive comparison against "Payment ID" would miss it entirely.
        let mut screens = nbgl_screens(RECEIVER);
        screens[1] = screens[1].replace("Reject2 of 3", "Payment ID32 bytesReject2 of 3");
        let problems = expected
            .check(UiToolkit::Nbgl, &screens)
            .expect_err("an unexpected Payment ID row must fail");
        assert!(problems.iter().any(|p| p.contains("Payment ID")), "{problems:?}");

        let mut screens = bagl_screens(&as_bagl_reports(RECEIVER));
        screens.insert(4, as_bagl_reports("Payment ID32 bytes"));
        let problems = expected
            .check(UiToolkit::Bagl, &screens)
            .expect_err("an unexpected Payment ID row must fail on BAGL too");
        assert!(problems.iter().any(|p| p.contains("Payment ID")), "{problems:?}");
    }

    #[test]
    fn a_present_payment_id_row_is_asserted_present() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, RECEIVER, 32);
        assert!(expected.absent().is_empty());

        let mut screens = nbgl_screens(RECEIVER);
        screens[1] = screens[1].replace("Reject2 of 3", "Payment ID32 bytesReject2 of 3");
        expected.check(UiToolkit::Nbgl, &screens).unwrap();

        // Without the row, the same expectation must fail.
        assert!(expected.check(UiToolkit::Nbgl, &nbgl_screens(RECEIVER)).is_err());
    }

    #[test]
    fn an_empty_review_is_a_failure_and_not_a_pass() {
        let expected = ExpectedReview::one_sided_metadata_signature(12_345, RECEIVER, 0);
        let problems = expected
            .check(UiToolkit::Nbgl, &[])
            .expect_err("no screens at all must never pass, whatever is asserted absent");
        assert!(problems.iter().any(|p| p.contains("no text at all")), "{problems:?}");
    }

    #[test]
    fn page_headers_are_stripped_so_a_split_value_is_contiguous() {
        let names = vec!["Receiver".to_string()];
        assert_eq!(strip_page_headers("Receiver (1/2)abc", &names), "Receiverabc");
        assert_eq!(strip_page_headers("Receiver (2/2)def", &names), "def");
        // A single page field has no counter and must be left exactly as it is.
        assert_eq!(strip_page_headers("Receiverabc", &names), "Receiverabc");
        // Something that merely looks like a counter is not one.
        assert_eq!(strip_page_headers("Receiver (x/2)abc", &names), "Receiver (x/2)abc");
        assert_eq!(strip_page_headers("Receiver (1/2abc", &names), "Receiver (1/2abc");
    }

    #[test]
    fn a_ten_page_counter_is_parsed() {
        let names = vec!["Receiver".to_string()];
        assert_eq!(strip_page_headers("Receiver (10/12)abc", &names), "abc");
        assert_eq!(strip_page_headers("Receiver (1/12)abc", &names), "Receiverabc");
    }

    #[test]
    fn models_map_to_the_toolkit_the_device_application_compiles_for() {
        assert_eq!(DeviceModel::NanoSPlus.toolkit(), UiToolkit::Bagl);
        assert_eq!(DeviceModel::NanoX.toolkit(), UiToolkit::Bagl);
        assert_eq!(DeviceModel::Stax.toolkit(), UiToolkit::Nbgl);
        assert_eq!(DeviceModel::Flex.toolkit(), UiToolkit::Nbgl);
    }

    #[test]
    fn model_names_round_trip() {
        for model in DeviceModel::ALL {
            assert_eq!(DeviceModel::from_name(model.name()), Some(model));
        }
        assert_eq!(DeviceModel::from_name("nanosp"), None);
        assert_eq!(DeviceModel::from_name(""), None);
    }

    #[test]
    fn a_summary_names_every_field_and_its_value() {
        let expected = ExpectedReview::one_sided_metadata_signature(1_500_000, RECEIVER, 8);
        let summary = expected.summary();
        assert!(summary.contains("Amount: 1.50 T"), "{summary}");
        assert!(summary.contains(RECEIVER), "{summary}");
        assert!(summary.contains("Payment ID: 8 bytes"), "{summary}");
    }

    #[test]
    #[should_panic(expected = "there is no 'Nonsense' field")]
    fn corrupting_a_field_that_does_not_exist_panics() {
        let _wrong = ExpectedReview::one_sided_metadata_signature(1, RECEIVER, 0).with_field_value("Nonsense", "x");
    }
}
