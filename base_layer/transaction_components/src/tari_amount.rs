// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    convert::{TryFrom, TryInto},
    fmt::{Display, Error, Formatter},
    iter::Sum,
    ops::{Add, Div, DivAssign, Mul, MulAssign, Sub},
    str::FromStr,
};

use borsh::{BorshDeserialize, BorshSerialize};
use decimal_rs::{Decimal, DecimalConvertError};
use newtype_ops::newtype_ops;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use thiserror::Error as ThisError;

use super::format_currency;

/// All calculations using Tari amounts should use these newtypes to prevent bugs related to rounding errors, unit
/// conversion errors etc.
///
/// ```edition2021
/// use tari_transaction_components::MicroMinotari;
///
/// let a = MicroMinotari::from(500);
/// let b = MicroMinotari::from(50);
/// assert_eq!(a + b, MicroMinotari::from(550));
/// ```
#[derive(
    Copy,
    Default,
    Clone,
    Debug,
    Eq,
    Hash,
    PartialEq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]

/// The minimum spendable unit Tari token amount
pub struct MicroMinotari(pub u64);

#[derive(Debug, Clone, ThisError, PartialEq, Eq)]
pub enum MicroMinotariError {
    #[error("Failed to parse value: {0}")]
    ParseError(String),
    #[error("Failed to convert value: {0}")]
    ConversionError(DecimalConvertError),
}

// DecimalConvertError does not implement Error
impl From<DecimalConvertError> for MicroMinotariError {
    fn from(err: DecimalConvertError) -> Self {
        MicroMinotariError::ConversionError(err)
    }
}
/// A convenience constant that makes it easier to define Tari amounts.
/// ```edition2021
/// use tari_transaction_components::tari_amount::{MicroMinotari, T, uT};
/// assert_eq!(MicroMinotari::from(42), 42 * uT);
/// assert_eq!(1 * T, 1_000_000.into());
/// assert_eq!(3_000_000 * uT, 3 * T);
/// ```
#[allow(non_upper_case_globals)]
pub const uT: MicroMinotari = MicroMinotari(1);
pub const T: MicroMinotari = MicroMinotari(1_000_000);

// You can only add or subtract µT from µT
newtype_ops! { [MicroMinotari] {add sub mul div} {:=} Self Self }
newtype_ops! { [MicroMinotari] {add sub mul div} {:=} &Self &Self }
newtype_ops! { [MicroMinotari] {add sub mul div} {:=} Self &Self }

// Multiplication and division only makes sense when µT is multiplied/divided by a scalar
newtype_ops! { [MicroMinotari] {mul div rem} {:=} Self u64 }
newtype_ops! { [MicroMinotari] {mul div rem} {:=} &Self u64 }

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl Mul<MicroMinotari> for u64 {
    type Output = MicroMinotari;

    fn mul(self, rhs: MicroMinotari) -> Self::Output {
        MicroMinotari(self * rhs.0)
    }
}

impl MicroMinotari {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub fn checked_add<T>(&self, v: T) -> Option<MicroMinotari>
    where T: AsRef<MicroMinotari> {
        self.as_u64().checked_add(v.as_ref().as_u64()).map(Into::into)
    }

    pub fn checked_sub<T>(&self, v: T) -> Option<MicroMinotari>
    where T: AsRef<MicroMinotari> {
        self.as_u64().checked_sub(v.as_ref().as_u64()).map(Into::into)
    }

    pub fn checked_mul<T>(&self, v: T) -> Option<MicroMinotari>
    where T: AsRef<MicroMinotari> {
        self.as_u64().checked_mul(v.as_ref().as_u64()).map(Into::into)
    }

    pub fn checked_div<T>(&self, v: T) -> Option<MicroMinotari>
    where T: AsRef<MicroMinotari> {
        self.as_u64().checked_div(v.as_ref().as_u64()).map(Into::into)
    }

    pub fn saturating_sub<T>(&self, v: T) -> MicroMinotari
    where T: AsRef<MicroMinotari> {
        self.as_u64().saturating_sub(v.as_ref().as_u64()).into()
    }

    pub fn saturating_add<T>(&self, v: T) -> MicroMinotari
    where T: AsRef<MicroMinotari> {
        self.as_u64().saturating_add(v.as_ref().as_u64()).into()
    }

    pub fn saturating_mul<T>(&self, v: T) -> MicroMinotari
    where T: AsRef<MicroMinotari> {
        self.as_u64().saturating_mul(v.as_ref().as_u64()).into()
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn as_u128(&self) -> u128 {
        u128::from(self.0)
    }

    pub fn to_currency_string(&self, sep: char) -> String {
        format!("{} µT", format_currency(&self.as_u64().to_string(), sep))
    }
}

impl AsRef<MicroMinotari> for MicroMinotari {
    fn as_ref(&self) -> &MicroMinotari {
        self
    }
}

impl Display for MicroMinotari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        if *self < T {
            write!(f, "{} µT", self.as_u64())
        } else {
            Minotari::from(*self).fmt(f)
        }
    }
}

impl From<MicroMinotari> for u64 {
    fn from(v: MicroMinotari) -> Self {
        v.0
    }
}

/// Parses an amount: optional surrounding whitespace, digits, an optional `.` with 1-6 digits, at most one space and
/// an optional case-insensitive unit `T`, `uT`, `µT` or `μT`. Without a unit, the amount is µT when
/// `no_unit_integer_is_micro` is set and there is no `.`, otherwise T.
fn parse_amount(s: &str, no_unit_integer_is_micro: bool) -> Result<MicroMinotari, MicroMinotariError> {
    let grammar_error = || {
        MicroMinotariError::ParseError(
            "expected digits, an optional '.' with 1 to 6 decimals, and an optional unit T or µT".to_string(),
        )
    };
    if s.contains(',') {
        return Err(MicroMinotariError::ParseError(
            "',' is not allowed; the decimal separator is '.' and thousands separators are not supported".to_string(),
        ));
    }
    let s = s.trim();

    let int_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (int_digits, rest) = s.split_at(int_end);
    if int_digits.is_empty() {
        return Err(grammar_error());
    }

    let mut frac_digits = None;
    let mut rest = rest;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let frac_end = after_dot.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_dot.len());
        if frac_end == 0 || frac_end > 6 {
            return Err(grammar_error());
        }
        frac_digits = Some(&after_dot[..frac_end]);
        rest = &after_dot[frac_end..];
    }

    let unit = rest.strip_prefix(' ').unwrap_or(rest);
    if unit.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(MicroMinotariError::ParseError(
            "thousands separators are not supported".to_string(),
        ));
    }
    let unit = unit.to_ascii_lowercase();
    let is_micro = match unit.as_str() {
        "" => no_unit_integer_is_micro && frac_digits.is_none(),
        "t" => false,
        "ut" | "µt" | "μt" => true,
        _ => return Err(grammar_error()),
    };

    let overflow = || MicroMinotariError::from(DecimalConvertError::Overflow);
    let whole = int_digits.parse::<u64>().map_err(|_| overflow())?;
    if is_micro {
        if frac_digits.is_some() {
            return Err(MicroMinotariError::ParseError(
                "µT amounts cannot have decimals".to_string(),
            ));
        }
        return Ok(MicroMinotari(whole));
    }
    let mut micro = whole.checked_mul(1_000_000).ok_or_else(overflow)?;
    if let Some(frac) = frac_digits {
        let padded = format!("{frac:0<6}");
        let frac_micro = padded.parse::<u64>().map_err(|_| overflow())?;
        micro = micro.checked_add(frac_micro).ok_or_else(overflow)?;
    }
    Ok(MicroMinotari(micro))
}

/// Strict amount grammar, e.g. `5`, `5.25 T`, `100 µT`. Without a unit, an integer is µT and a value with a `.` is T.
impl FromStr for MicroMinotari {
    type Err = MicroMinotariError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_amount(s, true)
    }
}

impl From<u64> for MicroMinotari {
    fn from(v: u64) -> Self {
        MicroMinotari(v)
    }
}

impl From<MicroMinotari> for f64 {
    fn from(v: MicroMinotari) -> Self {
        v.0 as f64
    }
}

impl From<Minotari> for MicroMinotari {
    fn from(v: Minotari) -> Self {
        v.0
    }
}

impl From<MicroMinotari> for RistrettoSecretKey {
    fn from(v: MicroMinotari) -> Self {
        v.0.into()
    }
}

impl<'a> Sum<&'a MicroMinotari> for MicroMinotari {
    fn sum<I: Iterator<Item = &'a MicroMinotari>>(iter: I) -> MicroMinotari {
        iter.fold(MicroMinotari::from(0), Add::add)
    }
}

impl Sum<MicroMinotari> for MicroMinotari {
    fn sum<I: Iterator<Item = MicroMinotari>>(iter: I) -> MicroMinotari {
        iter.fold(MicroMinotari::from(0), Add::add)
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl Add<Minotari> for MicroMinotari {
    type Output = Self;

    fn add(self, rhs: Minotari) -> Self::Output {
        self + rhs.0
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl Sub<Minotari> for MicroMinotari {
    type Output = Self;

    fn sub(self, rhs: Minotari) -> Self::Output {
        self - rhs.0
    }
}

/// A convenience struct for representing full Tari.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd)]
pub struct Minotari(MicroMinotari);

newtype_ops! { [Minotari] {add sub mul div} {:=} Self Self }
newtype_ops! { [Minotari] {add sub mul div} {:=} &Self &Self }
newtype_ops! { [Minotari] {add sub mul div} {:=} Self &Self }

// You can only add or subtract µT from µT
newtype_ops! { [Minotari] {add sub mul div} {:=} Self MicroMinotari }
newtype_ops! { [Minotari] {add sub mul div} {:=} &Self &MicroMinotari }
newtype_ops! { [Minotari] {add sub mul div} {:=} Self &MicroMinotari }

impl Minotari {
    /// Attempts to convert an float into an _approximate_ Tari value. This function is "lossy" in that it only includes
    /// digits up to 6 decimal places. It also does not provide guarantees that the intended value is correctly
    /// represented as MicroMinotari e.g 1.555500 could be 15555499uT due to the decimal conversion. This function is
    /// only used for tests.
    #[cfg(test)]
    pub(self) fn try_from_f32_lossy(v: f32) -> Result<Self, MicroMinotariError> {
        let d = Decimal::try_from(v)?.trunc(6);
        d.try_into()
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Into::into)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Into::into)
    }

    pub fn checked_mul(self, other: Self) -> Option<Self> {
        self.0.checked_mul(other.0).map(Into::into)
    }

    pub fn checked_div(self, other: Self) -> Option<Self> {
        self.0.checked_div(other.0).map(Into::into)
    }

    pub fn to_currency_string(&self, sep: char) -> String {
        // UNWRAP: MAX_I128_REPR > u64::MAX and scale is within bounds (see Decimal::from_parts)
        let d = Decimal::from_parts(u128::from(self.0.as_u64()), 6, false).unwrap();
        format!("{} T", format_currency(&d.to_string(), sep))
    }

    #[allow(non_snake_case)]
    pub fn uT(&self) -> MicroMinotari {
        self.0
    }
}

impl From<MicroMinotari> for Minotari {
    fn from(v: MicroMinotari) -> Self {
        Self(v)
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl From<u64> for Minotari {
    fn from(v: u64) -> Self {
        Self((v * 1_000_000).into())
    }
}

impl TryFrom<Decimal> for Minotari {
    type Error = MicroMinotariError;

    /// Converts Decimal into Minotari up to the first 6 decimal values. This will return an error if:
    /// 1. the value is negative,
    /// 1. the value has more than 6 decimal places (scale > 6)
    /// 1. the value exceeds u64::MAX
    fn try_from(v: Decimal) -> Result<Self, Self::Error> {
        if v.is_sign_negative() {
            Err(MicroMinotariError::ParseError("value cannot be negative".to_string()))
        } else if v.scale() > 6 {
            Err(MicroMinotariError::ParseError(format!("too many decimals ({v})")))
        } else {
            let (micro_tari, _, _) = v
                .checked_mul(Decimal::from(1_000_000u64))
                .ok_or(DecimalConvertError::Overflow)?
                .trunc(0)
                .into_parts();
            let micro_tari = micro_tari.try_into().map_err(|_| DecimalConvertError::Overflow)?;
            Ok(Self(MicroMinotari(micro_tari)))
        }
    }
}

/// Strict amount grammar, e.g. `5`, `5.25 T`, `100 µT`. Without a unit, the value is T.
impl FromStr for Minotari {
    type Err = MicroMinotariError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_amount(s, false).map(Minotari::from)
    }
}

impl Display for Minotari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let d1 = Decimal::from(self.0.as_u64());
        let d2 = Decimal::try_from(1_000_000f64).expect("will succeed");
        let precision = f.precision().unwrap_or(6);
        // `d2` is a non-zero constant, so this division cannot fail.
        let value = d1.checked_div(d2).unwrap_or_default();
        write!(f, "{1:.*} T", precision, value)
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl Mul<u64> for Minotari {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self::Output {
        (self.0 * rhs).into()
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl MulAssign<u64> for Minotari {
    fn mul_assign(&mut self, rhs: u64) {
        self.0 *= rhs;
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl Div<u64> for Minotari {
    type Output = Self;

    fn div(self, rhs: u64) -> Self::Output {
        (self.0 / rhs).into()
    }
}

// These operator impls deliberately keep Rust's native overflow behaviour: the release profile
// sets `overflow-checks = true` precisely so that bad amount arithmetic panics instead of
// silently producing a wrong value. Callers that need to handle overflow use the
// `checked_add`/`checked_sub`/`checked_mul` helpers on `MicroMinotari`.
#[allow(clippy::arithmetic_side_effects)]
impl DivAssign<u64> for Minotari {
    fn div_assign(&mut self, rhs: u64) {
        self.0 /= rhs;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn micro_tari_arithmetic() {
        let v = 100 * uT + Minotari::from(99u64);
        assert_eq!(v, MicroMinotari(99_000_100));
        let v = Minotari::from(99u64) - 100 * uT;
        assert_eq!(v, MicroMinotari(98_999_900).into());
        let v = Minotari::from(99u64) * 100u64;
        assert_eq!(v, MicroMinotari(9_900_000_000).into());
        let v = Minotari::from(990u64) / 100u64;
        assert_eq!(v, MicroMinotari(9_900_000).into());

        let mut a = MicroMinotari::from(500);
        let b = MicroMinotari::from(50);
        assert_eq!(a + b, MicroMinotari::from(550));
        assert_eq!(a - b, MicroMinotari::from(450));
        assert_eq!(a * 5, MicroMinotari::from(2_500));
        assert_eq!(a / 10, MicroMinotari::from(50));
        a += b;
        assert_eq!(a, MicroMinotari::from(550));
        a -= MicroMinotari::from(45);
        assert_eq!(a, MicroMinotari::from(505));
        assert_eq!(a % 50, MicroMinotari::from(5));
    }

    #[test]
    fn micro_tari_display() {
        let s = format!("{}", MicroMinotari::from(1234));
        assert_eq!(s, "1234 µT");
        let s = format!("{}", Minotari::from(MicroMinotari::from(1_000_000)));
        assert_eq!(s, "1.000000 T");
        let s = format!("{}", MicroMinotari::from(99_100_000));
        assert_eq!(s, "99.100000 T");
        let s = format!("{}", MicroMinotari::from(1_000_000_000));
        assert_eq!(s, "1000.000000 T");

        let s = format!("{:.0}", MicroMinotari::from(1_000_000_000));
        assert_eq!(s, "1000 T");
    }

    #[test]
    fn formatted_micro_tari_display() {
        let s = MicroMinotari::from(99_100_000).to_currency_string(',');
        assert_eq!(s, "99,100,000 µT");
        let s = MicroMinotari::from(1_000_000_000).to_currency_string(',');
        assert_eq!(s, "1,000,000,000 µT");
        let s = format!("{:.2}", Minotari::try_from_f32_lossy(1.234).unwrap());
        assert_eq!(s, "1.23 T");
        let s = format!("{:.2}", Minotari::try_from_f32_lossy(99_999.1).unwrap());
        assert_eq!(s, "99999.10 T");
    }

    #[test]
    fn formatted_tari_display() {
        let s = Minotari::from(99_100_000).to_currency_string(',');
        assert_eq!(s, "99,100,000 T");
        let s = Minotari::from(1_000_000_000).to_currency_string(',');
        assert_eq!(s, "1,000,000,000 T");
    }

    #[test]
    fn micro_tari_from_string() {
        let micro_tari = MicroMinotari::from(99_100_000);
        let s = format!("{micro_tari}");
        assert_eq!(micro_tari, MicroMinotari::from_str(s.as_str()).unwrap());
        let tari = Minotari::try_from_f32_lossy(1.12).unwrap();
        let s = format!("{tari}");
        assert_eq!(MicroMinotari::from(tari), MicroMinotari::from_str(s.as_str()).unwrap());
        assert_eq!(
            MicroMinotari::from(5_000_000),
            MicroMinotari::from_str("5000000").unwrap()
        );
        assert_eq!(
            MicroMinotari::from(5_000_000),
            MicroMinotari::from_str("5000000 uT").unwrap()
        );
        assert_eq!(MicroMinotari::from(5_000_000), MicroMinotari::from_str("5 T").unwrap());
        assert!(MicroMinotari::from_str("-5 T").is_err());
        assert!(MicroMinotari::from_str("-5 uT").is_err());
        assert!(MicroMinotari::from_str("5garbage T").is_err());
    }

    #[test]
    fn amount_grammar_accepts() {
        let cases = [
            ("5", 5),
            ("5.5", 5_500_000),
            ("  7 T  ", 7_000_000),
            ("7T", 7_000_000),
            ("7 t", 7_000_000),
            ("0.000001 T", 1),
            ("1.123456", 1_123_456),
            ("42 uT", 42),
            ("42UT", 42),
            ("42 \u{b5}T", 42),
            ("42 \u{3bc}T", 42),
            ("42 \u{3bc}t", 42),
            ("007", 7),
        ];
        for (input, micro) in cases {
            assert_eq!(MicroMinotari::from_str(input), Ok(MicroMinotari(micro)), "{input}");
        }
        // Minotari defaults to T without a unit
        assert_eq!(Minotari::from_str("5"), Ok(Minotari::from(5u64)));
        assert_eq!(Minotari::from_str("5.5"), Ok(MicroMinotari(5_500_000).into()));
        assert_eq!(Minotari::from_str("5 uT"), Ok(MicroMinotari(5).into()));
    }

    #[test]
    fn amount_display_roundtrips() {
        for micro in [0, 1, 999_999, 1_500_000, 123_456_789_012, u64::MAX] {
            let m = MicroMinotari(micro);
            assert_eq!(MicroMinotari::from_str(&m.to_string()), Ok(m));
            let t = Minotari::from(m);
            assert_eq!(Minotari::from_str(&t.to_string()), Ok(t));
        }
        assert_eq!(MicroMinotari::from_str("1.500000 T"), Ok(MicroMinotari(1_500_000)));
        assert_eq!(MicroMinotari::from_str("1500000 µT"), Ok(MicroMinotari(1_500_000)));
        assert_eq!(Minotari::from_str("1.500000 T"), Ok(MicroMinotari(1_500_000).into()));
        assert_eq!(Minotari::from_str("1500000 µT"), Ok(MicroMinotari(1_500_000).into()));
    }

    #[test]
    fn amount_grammar_rejects() {
        let cases = [
            "1,5 T",
            "0,001 T",
            "2t5",
            "1 000",
            "1,000,000 T",
            "1.5 uT",
            "1.5 µT",
            "1.1234567 T",
            "",
            "   ",
            "T",
            "uT",
            ".5",
            "5.",
            "5. T",
            "5  T",
            "+5",
            "-5",
            "5 T T",
            "5 Tari",
            "5\tT",
            "1e6",
            "18446744073709551616",
            "18446744073709551616 uT",
            "18446744073709.551616 T",
            "99999999999999999999 T",
            "5 \u{39c}T",
            "5 \u{ff34}",
        ];
        for input in cases {
            assert!(
                MicroMinotari::from_str(input).is_err(),
                "MicroMinotari accepted {input:?}"
            );
            assert!(Minotari::from_str(input).is_err(), "Minotari accepted {input:?}");
        }
        let err = MicroMinotari::from_str("1,5 T").unwrap_err().to_string();
        assert!(err.contains("decimal separator is '.'"), "{err}");
        let err = MicroMinotari::from_str("1 000 000").unwrap_err().to_string();
        assert!(err.contains("thousands separators"), "{err}");
    }

    #[test]
    #[allow(non_snake_case)]
    fn add_tari_and_micro_Minotari() {
        let a = MicroMinotari::from(100_000);
        let b = Minotari::try_from_f32_lossy(0.23).unwrap();
        let sum: Minotari = b + a;
        assert_eq!(sum, Minotari::try_from_f32_lossy(0.33).unwrap());
    }

    #[test]
    fn tari_arithmetic() {
        let mut a = Minotari::try_from_f32_lossy(1.5).unwrap();
        let b = Minotari::try_from_f32_lossy(2.25).unwrap();
        assert_eq!(a + b, Minotari::try_from_f32_lossy(3.75).unwrap());
        assert_eq!(a.checked_sub(b), None);
        // Negative values are not currently used and not supported, adding support would be fairly straight forward
        // Currently, this panics with an underflow
        // assert_eq!(a - b, Tari::from_f32_lossy(-0.75).unwrap());
        assert_eq!(a * 10, Minotari::try_from_f32_lossy(15.0).unwrap());
        assert_eq!(b / 2, Minotari::try_from_f32_lossy(1.125).unwrap());
        a += b;
        assert_eq!(a, Minotari::try_from_f32_lossy(3.75).unwrap());
        a -= Minotari::try_from_f32_lossy(0.75).unwrap();
        assert_eq!(a, Minotari::try_from_f32_lossy(3.0).unwrap());
    }

    #[test]
    fn tari_display() {
        let s = format!(
            "{}",
            // Decimal is created with a scale > 3 if we dont round (1.233999999999..)
            Minotari::try_from(Decimal::try_from(1.234).unwrap().round(3)).unwrap()
        );
        assert_eq!(s, "1.234000 T");
        let s = format!(
            "{}",
            Minotari::try_from(Decimal::try_from(99.100).unwrap().round(3)).unwrap()
        );
        assert_eq!(s, "99.100000 T");
    }

    #[test]
    fn to_string_from_string_max_conversion() {
        let max_value = MicroMinotari(u64::MAX);

        assert_eq!(max_value.as_u64().to_string(), "18446744073709551615");
        let max_str_with_currency = format!("{max_value}");
        assert_eq!(&max_str_with_currency, "18446744073709.551615 T");
        let max_str_no_currency = max_str_with_currency[0..max_str_with_currency.len() - 2].to_string();
        assert_eq!(&max_str_no_currency, "18446744073709.551615");

        assert_eq!(max_value, MicroMinotari::from_str(&max_str_with_currency).unwrap());
        assert_eq!(max_value, MicroMinotari::from_str(&max_str_no_currency).unwrap());
        assert_eq!(
            Minotari::from(max_value),
            Minotari::from_str(&max_str_with_currency).unwrap()
        );
        assert_eq!(
            Minotari::from(max_value),
            Minotari::from_str(&max_str_no_currency).unwrap()
        );

        assert!(MicroMinotari::from_str("18446744073709.551615 T").is_ok());
        assert!(MicroMinotari::from_str("18446744073709.551615 uT").is_err());
        assert!(MicroMinotari::from_str("18446744073709.551615T").is_ok());
        assert!(MicroMinotari::from_str("18446744073709.551615uT").is_err());
        assert!(MicroMinotari::from_str("18446744073709.551615").is_ok());
        assert!(MicroMinotari::from_str("18446744073709551615").is_ok());

        assert!(Minotari::from_str("18446744073709.551615 T").is_ok());
        assert!(Minotari::from_str("18446744073709.551615 uT").is_err());
        assert!(Minotari::from_str("18446744073709.551615T").is_ok());
        assert!(Minotari::from_str("18446744073709.551615uT").is_err());
        assert!(Minotari::from_str("18446744073709.551615").is_ok());
        assert_eq!(
            &Minotari::from_str("18446744073709551615").unwrap_err().to_string(),
            "Failed to convert value: numeric overflow"
        );
    }
}
