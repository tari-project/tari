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
    ops::{Add, Mul},
};

use decimal_rs::{Decimal, DecimalConvertError};
use derive_more::{Add, AddAssign, Div, From, FromStr, Into, Mul, Rem, Sub, SubAssign};
use newtype_ops::newtype_ops;
use serde::{Deserialize, Serialize};
use tari_crypto::ristretto::RistrettoSecretKey;
use thiserror::Error as ThisError;

use super::format_currency;

/// All calculations using Tari amounts should use these newtypes to prevent bugs related to rounding errors, unit
/// conversion errors etc.
///
/// ```edition2018
/// use tari_core::transactions::tari_amount::MicroTari;
///
/// let a = MicroTari::from(500);
/// let b = MicroTari::from(50);
/// assert_eq!(a + b, MicroTari::from(550));
/// ```
#[derive(Copy, Default, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MicroTari(pub u64);

#[derive(Debug, Clone, ThisError, PartialEq)]
pub enum MicroTariError {
    #[error("Failed to parse value: {0}")]
    ParseError(String),
    #[error("Failed to convert value: {0}")]
    ConversionError(#[from] DecimalConvertError),
}
/// A convenience constant that makes it easier to define Tari amounts.
/// ```edition2018
/// use tari_core::transactions::tari_amount::{uT, MicroTari, T};
/// assert_eq!(MicroTari::from(42), 42 * uT);
/// assert_eq!(1 * T, 1_000_000.into());
/// assert_eq!(3_000_000 * uT, 3 * T);
/// ```
#[allow(non_upper_case_globals)]
pub const uT: MicroTari = MicroTari(1);
pub const T: MicroTari = MicroTari(1_000_000);

// You can only add or subtract µT from µT
newtype_ops! { [MicroTari] {add sub mul div} {:=} Self Self }
newtype_ops! { [MicroTari] {add sub mul div} {:=} &Self &Self }
newtype_ops! { [MicroTari] {add sub mul div} {:=} Self &Self }

// Multiplication and division only makes sense when µT is multiplied/divided by a scalar
newtype_ops! { [MicroTari] {mul div rem} {:=} Self u64 }

impl Mul<MicroTari> for u64 {
    type Output = MicroTari;

    fn mul(self, rhs: MicroTari) -> Self::Output {
        MicroTari(self * rhs.0)
    }
}

impl MicroTari {
    pub fn checked_add(self, v: MicroTari) -> Option<MicroTari> {
        self.as_u64().checked_add(v.as_u64()).map(Into::into)
    }

    pub fn checked_sub(self, v: MicroTari) -> Option<MicroTari> {
        if self >= v {
            return Some(self - v);
        }
        None
    }

    pub fn saturating_sub(self, v: MicroTari) -> MicroTari {
        if self >= v {
            return self - v;
        }
        Self(0)
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn formatted(self) -> FormattedMicroTari {
        self.into()
    }
}

#[allow(clippy::identity_op)]
impl Display for MicroTari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        if *self < 1 * T {
            write!(f, "{} µT", self.0)
        } else {
            Tari::from(*self).fmt(f)
        }
    }
}

impl From<MicroTari> for u64 {
    fn from(v: MicroTari) -> Self {
        v.0
    }
}

impl std::str::FromStr for MicroTari {
    type Err = MicroTariError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Is this Tari or MicroTari
        let processed = s.replace(",", "").replace(" ", "").to_ascii_lowercase();
        let is_micro_tari = if processed.ends_with("ut") || processed.ends_with("µt") {
            true
        } else {
            !processed.ends_with('t')
        };

        // Avoid using f64 if we an
        let processed = processed.replace("ut", "").replace("µt", "").replace("t", "");
        if is_micro_tari {
            processed
                .parse::<u64>()
                .map(MicroTari::from)
                .map_err(|e| MicroTariError::ParseError(e.to_string()))
        } else {
            processed
                .parse::<Decimal>()
                .map_err(|e| MicroTariError::ParseError(e.to_string()))
                .map(|v| {
                    if v.is_sign_negative() {
                        Err(MicroTariError::ParseError("value cannot be negative".to_string()))
                    } else {
                        Tari::from(v).try_into().map_err(MicroTariError::from)
                    }
                })?
        }
    }
}

impl TryFrom<Tari> for MicroTari {
    type Error = DecimalConvertError;

    fn try_from(v: Tari) -> Result<Self, Self::Error> {
        (v.0 * 1_000_000u32).try_into().map(Self)
    }
}

impl From<u64> for MicroTari {
    fn from(v: u64) -> Self {
        MicroTari(v)
    }
}

impl From<MicroTari> for f64 {
    fn from(v: MicroTari) -> Self {
        v.0 as f64
    }
}

impl From<MicroTari> for RistrettoSecretKey {
    fn from(v: MicroTari) -> Self {
        v.0.into()
    }
}

impl<'a> Sum<&'a MicroTari> for MicroTari {
    fn sum<I: Iterator<Item = &'a MicroTari>>(iter: I) -> MicroTari {
        iter.fold(MicroTari::from(0), Add::add)
    }
}

impl Sum<MicroTari> for MicroTari {
    fn sum<I: Iterator<Item = MicroTari>>(iter: I) -> MicroTari {
        iter.fold(MicroTari::from(0), Add::add)
    }
}

#[derive(Copy, Default, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FormattedMicroTari(pub u64);

impl From<MicroTari> for FormattedMicroTari {
    fn from(v: MicroTari) -> Self {
        FormattedMicroTari(v.0)
    }
}

impl Display for FormattedMicroTari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let value = format!("{}", self.0);
        let formatted = format_currency(&value, ',');
        f.write_str(&formatted)?;
        f.write_str(" µT")?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct FormattedTari(pub Decimal);

impl From<Tari> for FormattedTari {
    fn from(v: Tari) -> Self {
        FormattedTari(v.0)
    }
}

impl Display for FormattedTari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let value = format!("{:.2}", self.0);
        let formatted = format_currency(&value, ',');
        f.write_str(&formatted)?;
        f.write_str(" T")?;
        Ok(())
    }
}

/// A convenience struct for representing full Tari. You should **never** use Tari in consensus calculations, because
/// Tari wraps a floating point value. Use MicroTari for that instead.
#[derive(
    Copy, Clone, Debug, PartialEq, PartialOrd, Add, AddAssign, Sub, SubAssign, Mul, Div, Rem, From, Into, FromStr,
)]
pub struct Tari(Decimal);

impl Tari {
    pub fn formatted(self) -> FormattedTari {
        self.into()
    }
}

impl Display for Tari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{:0.6} T", self.0)
    }
}

pub type TariConversionError = DecimalConvertError;

// TODO: Remove `f64` completely! Using it is the bad idea in general. #LOGGED
impl TryFrom<f64> for Tari {
    type Error = TariConversionError;

    fn try_from(v: f64) -> Result<Self, Self::Error> {
        Decimal::try_from(v).map(Self)
    }
}

impl From<MicroTari> for Tari {
    fn from(v: MicroTari) -> Self {
        Self(Decimal::from(v.0) / 1_000_000)
    }
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, str::FromStr};

    use super::{MicroTari, Tari};

    #[test]
    fn micro_tari_arithmetic() {
        let mut a = MicroTari::from(500);
        let b = MicroTari::from(50);
        assert_eq!(a + b, MicroTari::from(550));
        assert_eq!(a - b, MicroTari::from(450));
        assert_eq!(a * 5, MicroTari::from(2_500));
        assert_eq!(a / 10, MicroTari::from(50));
        a += b;
        assert_eq!(a, MicroTari::from(550));
        a -= MicroTari::from(45);
        assert_eq!(a, MicroTari::from(505));
        assert_eq!(a % 50, MicroTari::from(5));
    }

    #[test]
    fn micro_tari_display() {
        let s = format!("{}", MicroTari::from(1234));
        assert_eq!(s, "1234 µT");
        let s = format!("{}", MicroTari::from(1_000_000));
        assert_eq!(s, "1.000000 T");
    }

    #[test]
    fn formatted_micro_tari_display() {
        let s = format!("{}", MicroTari::from(99_100_000).formatted());
        assert_eq!(s, "99,100,000 µT");
        let s = format!("{}", MicroTari::from(1_000_000_000).formatted());
        assert_eq!(s, "1,000,000,000 µT");
    }

    #[test]
    fn micro_tari_from_string() {
        let micro_tari = MicroTari::from(99_100_000);
        let s = format!("{}", micro_tari.formatted());
        assert_eq!(micro_tari, MicroTari::from_str(s.as_str()).unwrap());
        let tari = Tari::try_from(1.12).unwrap();
        let s = format!("{}", tari.formatted());
        assert_eq!(
            MicroTari::try_from(tari).unwrap(),
            MicroTari::from_str(s.as_str()).unwrap()
        );
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5000000").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5,000,000").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5,000,000 uT").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5000000 uT").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5 T").unwrap());
        assert!(MicroTari::from_str("-5 T").is_err());
        assert!(MicroTari::from_str("-5 uT").is_err());
        assert!(MicroTari::from_str("5garbage T").is_err());
    }

    /// With `Decimal` the test with floats is not valid anymore:
    /// ```
    /// thread 'transactions::tari_amount::test::add_tari_and_microtari' panicked at 'assertion failed: `(left == right)`
    /// left: `Tari(Decimal { int_val: 33000000000000001000000000000000000000, scale: 38, negative: false })`,
    /// right: `Tari(Decimal { int_val: 33000000000000002, scale: 17, negative: false })`',
    /// ```
    #[test]
    fn add_tari_and_microtari() {
        let a = MicroTari::from(100_000);
        let b = Tari::from_str("0.23").unwrap();
        let sum: Tari = b + a.into();
        assert_eq!(sum, Tari::from_str("0.33").unwrap());
    }

    #[test]
    fn tari_arithmetic() {
        let mut a = Tari::try_from(1.5).unwrap();
        let b = Tari::try_from(2.25).unwrap();
        assert_eq!(a + b, Tari::try_from(3.75).unwrap());
        assert_eq!(a - b, Tari::try_from(-0.75).unwrap());
        assert_eq!(a * 10.0, Tari::try_from(15.0).unwrap());
        assert_eq!(b / 2.0, Tari::try_from(1.125).unwrap());
        a += b;
        assert_eq!(a, Tari::try_from(3.75).unwrap());
        a -= Tari::try_from(0.75).unwrap();
        assert_eq!(a, Tari::try_from(3.0).unwrap());
    }

    #[test]
    fn tari_display() {
        let s = format!("{}", Tari::try_from(1.234).unwrap());
        assert_eq!(s, "1.234000 T");
        let s = format!("{}", Tari::try_from(99.100).unwrap());
        assert_eq!(s, "99.100000 T");
    }

    #[test]
    fn formatted_tari_display() {
        let s = format!("{}", Tari::try_from(1.234).unwrap().formatted());
        assert_eq!(s, "1.23 T");
        let s = format!("{}", Tari::try_from(99999.100).unwrap().formatted());
        assert_eq!(s, "99,999.10 T");
    }
}
