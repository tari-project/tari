// Copyright 2019. The Taiji Project
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

/// All calculations using Taiji amounts should use these newtypes to prevent bugs related to rounding errors, unit
/// conversion errors etc.
///
/// ```edition2018
/// use taiji_core::transactions::taiji_amount::MicroMinotaiji;
///
/// let a = MicroMinotaiji::from(500);
/// let b = MicroMinotaiji::from(50);
/// assert_eq!(a + b, MicroMinotaiji::from(550));
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

/// The minimum spendable unit Taiji token amount
pub struct MicroMinotaiji(pub u64);

#[derive(Debug, Clone, ThisError, PartialEq, Eq)]
pub enum MicroMinotaijiError {
    #[error("Failed to parse value: {0}")]
    ParseError(String),
    #[error("Failed to convert value: {0}")]
    ConversionError(DecimalConvertError),
}

// DecimalConvertError does not implement Error
impl From<DecimalConvertError> for MicroMinotaijiError {
    fn from(err: DecimalConvertError) -> Self {
        MicroMinotaijiError::ConversionError(err)
    }
}
/// A convenience constant that makes it easier to define Taiji amounts.
/// ```edition2018
/// use taiji_core::transactions::taiji_amount::{uT, MicroMinotaiji, T};
/// assert_eq!(MicroMinotaiji::from(42), 42 * uT);
/// assert_eq!(1 * T, 1_000_000.into());
/// assert_eq!(3_000_000 * uT, 3 * T);
/// ```
#[allow(non_upper_case_globals)]
pub const uT: MicroMinotaiji = MicroMinotaiji(1);
pub const T: MicroMinotaiji = MicroMinotaiji(1_000_000);

// You can only add or subtract µT from µT
newtype_ops! { [MicroMinotaiji] {add sub mul div} {:=} Self Self }
newtype_ops! { [MicroMinotaiji] {add sub mul div} {:=} &Self &Self }
newtype_ops! { [MicroMinotaiji] {add sub mul div} {:=} Self &Self }

// Multiplication and division only makes sense when µT is multiplied/divided by a scalar
newtype_ops! { [MicroMinotaiji] {mul div rem} {:=} Self u64 }
newtype_ops! { [MicroMinotaiji] {mul div rem} {:=} &Self u64 }

impl Mul<MicroMinotaiji> for u64 {
    type Output = MicroMinotaiji;

    fn mul(self, rhs: MicroMinotaiji) -> Self::Output {
        MicroMinotaiji(self * rhs.0)
    }
}

impl MicroMinotaiji {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub fn checked_add(self, v: MicroMinotaiji) -> Option<MicroMinotaiji> {
        self.as_u64().checked_add(v.as_u64()).map(Into::into)
    }

    pub fn checked_sub(self, v: MicroMinotaiji) -> Option<MicroMinotaiji> {
        if self >= v {
            return Some(self - v);
        }
        None
    }

    pub fn checked_mul(self, v: MicroMinotaiji) -> Option<MicroMinotaiji> {
        self.as_u64().checked_mul(v.as_u64()).map(Into::into)
    }

    pub fn checked_div(self, v: MicroMinotaiji) -> Option<MicroMinotaiji> {
        self.as_u64().checked_div(v.as_u64()).map(Into::into)
    }

    pub fn saturating_sub(self, v: MicroMinotaiji) -> MicroMinotaiji {
        if self >= v {
            return self - v;
        }
        Self(0)
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn to_currency_string(&self, sep: char) -> String {
        format!("{} µT", format_currency(&self.as_u64().to_string(), sep))
    }
}

#[allow(clippy::identity_op)]
impl Display for MicroMinotaiji {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        if *self < 1 * T {
            write!(f, "{} µT", self.as_u64())
        } else {
            Minotaiji::from(*self).fmt(f)
        }
    }
}

impl From<MicroMinotaiji> for u64 {
    fn from(v: MicroMinotaiji) -> Self {
        v.0
    }
}

impl std::str::FromStr for MicroMinotaiji {
    type Err = MicroMinotaijiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let processed = s.replace([',', ' '], "").to_ascii_lowercase();
        // Is this Taiji or MicroMinotaiji
        let is_micro_taiji = if processed.ends_with("ut") || processed.ends_with("µt") {
            true
        } else {
            !processed.ends_with('t')
        };

        let processed = processed.replace("ut", "").replace("µt", "").replace('t', "");
        if is_micro_taiji {
            processed
                .parse::<u64>()
                .map(MicroMinotaiji::from)
                .map_err(|e| MicroMinotaijiError::ParseError(e.to_string()))
        } else {
            processed
                .parse::<Decimal>()
                .map_err(|e| MicroMinotaijiError::ParseError(e.to_string()))
                .and_then(Minotaiji::try_from)
                .map(MicroMinotaiji::from)
        }
    }
}

impl From<u64> for MicroMinotaiji {
    fn from(v: u64) -> Self {
        MicroMinotaiji(v)
    }
}

impl From<MicroMinotaiji> for f64 {
    fn from(v: MicroMinotaiji) -> Self {
        v.0 as f64
    }
}

impl From<Minotaiji> for MicroMinotaiji {
    fn from(v: Minotaiji) -> Self {
        v.0
    }
}

impl From<MicroMinotaiji> for RistrettoSecretKey {
    fn from(v: MicroMinotaiji) -> Self {
        v.0.into()
    }
}

impl<'a> Sum<&'a MicroMinotaiji> for MicroMinotaiji {
    fn sum<I: Iterator<Item = &'a MicroMinotaiji>>(iter: I) -> MicroMinotaiji {
        iter.fold(MicroMinotaiji::from(0), Add::add)
    }
}

impl Sum<MicroMinotaiji> for MicroMinotaiji {
    fn sum<I: Iterator<Item = MicroMinotaiji>>(iter: I) -> MicroMinotaiji {
        iter.fold(MicroMinotaiji::from(0), Add::add)
    }
}

impl Add<Minotaiji> for MicroMinotaiji {
    type Output = Self;

    fn add(self, rhs: Minotaiji) -> Self::Output {
        self + rhs.0
    }
}

impl Sub<Minotaiji> for MicroMinotaiji {
    type Output = Self;

    fn sub(self, rhs: Minotaiji) -> Self::Output {
        self - rhs.0
    }
}

/// A convenience struct for representing full Taiji.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd)]
pub struct Minotaiji(MicroMinotaiji);

newtype_ops! { [Minotaiji] {add sub mul div} {:=} Self Self }
newtype_ops! { [Minotaiji] {add sub mul div} {:=} &Self &Self }
newtype_ops! { [Minotaiji] {add sub mul div} {:=} Self &Self }

// You can only add or subtract µT from µT
newtype_ops! { [Minotaiji] {add sub mul div} {:=} Self MicroMinotaiji }
newtype_ops! { [Minotaiji] {add sub mul div} {:=} &Self &MicroMinotaiji }
newtype_ops! { [Minotaiji] {add sub mul div} {:=} Self &MicroMinotaiji }

impl Minotaiji {
    /// Attempts to convert an float into an _approximate_ Taiji value. This function is "lossy" in that it only includes
    /// digits up to 6 decimal places. It also does not provide guarantees that the intended value is correctly
    /// represented as MicroMinotaiji e.g 1.555500 could be 15555499uT due to the decimal conversion. This function is
    /// only used for tests.
    #[cfg(test)]
    pub(self) fn try_from_f32_lossy(v: f32) -> Result<Self, MicroMinotaijiError> {
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
}

impl From<MicroMinotaiji> for Minotaiji {
    fn from(v: MicroMinotaiji) -> Self {
        Self(v)
    }
}

impl From<u64> for Minotaiji {
    fn from(v: u64) -> Self {
        Self((v * 1_000_000).into())
    }
}

impl TryFrom<Decimal> for Minotaiji {
    type Error = MicroMinotaijiError;

    /// Converts Decimal into Minotaiji up to the first 6 decimal values. This will return an error if:
    /// 1. the value is negative,
    /// 1. the value has more than 6 decimal places (scale > 6)
    /// 1. the value exceeds u64::MAX
    fn try_from(v: Decimal) -> Result<Self, Self::Error> {
        if v.is_sign_negative() {
            Err(MicroMinotaijiError::ParseError("value cannot be negative".to_string()))
        } else if v.scale() > 6 {
            Err(MicroMinotaijiError::ParseError(format!("too many decimals ({})", v)))
        } else {
            let (micro_taiji, _, _) = (v * 1_000_000u64).trunc(0).into_parts();
            let micro_taiji = micro_taiji.try_into().map_err(|_| DecimalConvertError::Overflow)?;
            Ok(Self(MicroMinotaiji(micro_taiji)))
        }
    }
}

impl FromStr for Minotaiji {
    type Err = MicroMinotaijiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let d = Decimal::from_str(s).map_err(|e| MicroMinotaijiError::ParseError(e.to_string()))?;
        Self::try_from(d)
    }
}

impl Display for Minotaiji {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        // User can choose decimal precision, but default is 6
        let precision = f.precision().unwrap_or(6);
        write!(f, "{1:.*} T", precision, self.0.as_u64() as f64 / 1_000_000f64)
    }
}

impl Mul<u64> for Minotaiji {
    type Output = Self;

    fn mul(self, rhs: u64) -> Self::Output {
        (self.0 * rhs).into()
    }
}

impl MulAssign<u64> for Minotaiji {
    fn mul_assign(&mut self, rhs: u64) {
        self.0 *= rhs;
    }
}

impl Div<u64> for Minotaiji {
    type Output = Self;

    fn div(self, rhs: u64) -> Self::Output {
        (self.0 / rhs).into()
    }
}

impl DivAssign<u64> for Minotaiji {
    fn div_assign(&mut self, rhs: u64) {
        self.0 /= rhs;
    }
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, str::FromStr};

    use super::*;

    #[test]
    fn micro_taiji_arithmetic() {
        let v = 100 * uT + Minotaiji::from(99u64);
        assert_eq!(v, MicroMinotaiji(99_000_100));
        let v = Minotaiji::from(99u64) - 100 * uT;
        assert_eq!(v, MicroMinotaiji(98_999_900).into());
        let v = Minotaiji::from(99u64) * 100u64;
        assert_eq!(v, MicroMinotaiji(9_900_000_000).into());
        let v = Minotaiji::from(990u64) / 100u64;
        assert_eq!(v, MicroMinotaiji(9_900_000).into());

        let mut a = MicroMinotaiji::from(500);
        let b = MicroMinotaiji::from(50);
        assert_eq!(a + b, MicroMinotaiji::from(550));
        assert_eq!(a - b, MicroMinotaiji::from(450));
        assert_eq!(a * 5, MicroMinotaiji::from(2_500));
        assert_eq!(a / 10, MicroMinotaiji::from(50));
        a += b;
        assert_eq!(a, MicroMinotaiji::from(550));
        a -= MicroMinotaiji::from(45);
        assert_eq!(a, MicroMinotaiji::from(505));
        assert_eq!(a % 50, MicroMinotaiji::from(5));
    }

    #[test]
    fn micro_taiji_display() {
        let s = format!("{}", MicroMinotaiji::from(1234));
        assert_eq!(s, "1234 µT");
        let s = format!("{}", Minotaiji::from(MicroMinotaiji::from(1_000_000)));
        assert_eq!(s, "1.000000 T");
        let s = format!("{}", MicroMinotaiji::from(99_100_000));
        assert_eq!(s, "99.100000 T");
        let s = format!("{}", MicroMinotaiji::from(1_000_000_000));
        assert_eq!(s, "1000.000000 T");

        let s = format!("{:.0}", MicroMinotaiji::from(1_000_000_000));
        assert_eq!(s, "1000 T");
    }

    #[test]
    fn formatted_micro_taiji_display() {
        let s = MicroMinotaiji::from(99_100_000).to_currency_string(',');
        assert_eq!(s, "99,100,000 µT");
        let s = MicroMinotaiji::from(1_000_000_000).to_currency_string(',');
        assert_eq!(s, "1,000,000,000 µT");
        let s = format!("{:.2}", Minotaiji::try_from_f32_lossy(1.234).unwrap());
        assert_eq!(s, "1.23 T");
        let s = format!("{:.2}", Minotaiji::try_from_f32_lossy(99_999.1).unwrap());
        assert_eq!(s, "99999.10 T");
    }

    #[test]
    fn formatted_taiji_display() {
        let s = Minotaiji::from(99_100_000).to_currency_string(',');
        assert_eq!(s, "99,100,000 T");
        let s = Minotaiji::from(1_000_000_000).to_currency_string(',');
        assert_eq!(s, "1,000,000,000 T");
    }

    #[test]
    fn micro_taiji_from_string() {
        let micro_taiji = MicroMinotaiji::from(99_100_000);
        let s = format!("{}", micro_taiji);
        assert_eq!(micro_taiji, MicroMinotaiji::from_str(s.as_str()).unwrap());
        let taiji = Minotaiji::try_from_f32_lossy(1.12).unwrap();
        let s = format!("{}", taiji);
        assert_eq!(MicroMinotaiji::from(taiji), MicroMinotaiji::from_str(s.as_str()).unwrap());
        assert_eq!(
            MicroMinotaiji::from(5_000_000),
            MicroMinotaiji::from_str("5000000").unwrap()
        );
        assert_eq!(
            MicroMinotaiji::from(5_000_000),
            MicroMinotaiji::from_str("5,000,000").unwrap()
        );
        assert_eq!(
            MicroMinotaiji::from(5_000_000),
            MicroMinotaiji::from_str("5,000,000 uT").unwrap()
        );
        assert_eq!(
            MicroMinotaiji::from(5_000_000),
            MicroMinotaiji::from_str("5000000 uT").unwrap()
        );
        assert_eq!(MicroMinotaiji::from(5_000_000), MicroMinotaiji::from_str("5 T").unwrap());
        assert!(MicroMinotaiji::from_str("-5 T").is_err());
        assert!(MicroMinotaiji::from_str("-5 uT").is_err());
        assert!(MicroMinotaiji::from_str("5garbage T").is_err());
    }

    #[test]
    #[allow(non_snake_case)]
    fn add_taiji_and_micro_Minotaiji() {
        let a = MicroMinotaiji::from(100_000);
        let b = Minotaiji::try_from_f32_lossy(0.23).unwrap();
        let sum: Minotaiji = b + a;
        assert_eq!(sum, Minotaiji::try_from_f32_lossy(0.33).unwrap());
    }

    #[test]
    fn taiji_arithmetic() {
        let mut a = Minotaiji::try_from_f32_lossy(1.5).unwrap();
        let b = Minotaiji::try_from_f32_lossy(2.25).unwrap();
        assert_eq!(a + b, Minotaiji::try_from_f32_lossy(3.75).unwrap());
        assert_eq!(a.checked_sub(b), None);
        // Negative values are not currently used and not supported, adding support would be fairly straight forward
        // Currently, this panics with an underflow
        // assert_eq!(a - b, Taiji::from_f32_lossy(-0.75).unwrap());
        assert_eq!(a * 10, Minotaiji::try_from_f32_lossy(15.0).unwrap());
        assert_eq!(b / 2, Minotaiji::try_from_f32_lossy(1.125).unwrap());
        a += b;
        assert_eq!(a, Minotaiji::try_from_f32_lossy(3.75).unwrap());
        a -= Minotaiji::try_from_f32_lossy(0.75).unwrap();
        assert_eq!(a, Minotaiji::try_from_f32_lossy(3.0).unwrap());
    }

    #[test]
    fn taiji_display() {
        let s = format!(
            "{}",
            // Decimal is created with a scale > 3 if we dont round (1.233999999999..)
            Minotaiji::try_from(Decimal::try_from(1.234).unwrap().round(3)).unwrap()
        );
        assert_eq!(s, "1.234000 T");
        let s = format!(
            "{}",
            Minotaiji::try_from(Decimal::try_from(99.100).unwrap().round(3)).unwrap()
        );
        assert_eq!(s, "99.100000 T");
    }
}
