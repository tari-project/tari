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

use newtype_ops::newtype_ops;
use serde::{Deserialize, Serialize};

use crate::transactions::helpers::display_currency;
use std::{
    fmt::{Display, Error, Formatter},
    iter::Sum,
    ops::{Add, Mul},
};
use tari_crypto::ristretto::RistrettoSecretKey;
use thiserror::Error as ThisError;

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
}
/// A convenience constant that makes it easier to define Tari amounts.
/// ```edition2018
///   use tari_core::transactions::tari_amount::{MicroTari, uT, T};
///   assert_eq!(MicroTari::from(42), 42 * uT);
///   assert_eq!(1 * T, 1_000_000.into());
///   assert_eq!(3_000_000 * uT, 3 * T);
/// ```
#[allow(non_upper_case_globals)]
pub const uT: MicroTari = MicroTari(1);
pub const T: MicroTari = MicroTari(1_000_000);

// You can only add or subtract µT from µT
newtype_ops! { [MicroTari] {add sub} {:=} Self Self }
newtype_ops! { [MicroTari] {add sub} {:=} &Self &Self }
newtype_ops! { [MicroTari] {add sub} {:=} Self &Self }

// Multiplication and division only makes sense when µT is multiplied/divided by a scalar
newtype_ops! { [MicroTari] {mul div rem} {:=} Self u64 }

impl Mul<MicroTari> for u64 {
    type Output = MicroTari;

    fn mul(self, rhs: MicroTari) -> Self::Output {
        MicroTari(self * rhs.0)
    }
}

impl MicroTari {
    pub fn checked_sub(self, v: MicroTari) -> Option<MicroTari> {
        if self.0 >= v.0 {
            return Some(self - v);
        }
        None
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
                .map(|v| MicroTari::from(v.max(0)))
                .map_err(|e| MicroTariError::ParseError(e.to_string()))
        } else {
            processed
                .parse::<f64>()
                .map_err(|e| MicroTariError::ParseError(e.to_string()))
                .map(|v| {
                    if v < 0.0 {
                        Err(MicroTariError::ParseError("value cannot be negative".to_string()))
                    } else {
                        Ok(MicroTari::from(Tari::from(v.max(0.0))))
                    }
                })?
        }
    }
}

impl From<Tari> for MicroTari {
    fn from(v: Tari) -> Self {
        MicroTari((v.0 * 1e6) as u64)
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
        write!(f, "{} µT", display_currency(self.0 as f64, 0, ","))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct FormattedTari(pub f64);

impl From<Tari> for FormattedTari {
    fn from(v: Tari) -> Self {
        FormattedTari(v.0)
    }
}

impl Display for FormattedTari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{} T", display_currency(self.0, 2, ","))
    }
}

/// A convenience struct for representing full Tari. You should **never** use Tari in consensus calculations, because
/// Tari wraps a floating point value. Use MicroTari for that instead.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Tari(f64);

newtype_ops! { [Tari] {add sub} {:=} Self Self }
newtype_ops! { [Tari] {mul div rem} {:=} Self f64 }

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

impl From<Tari> for f64 {
    fn from(v: Tari) -> Self {
        v.0
    }
}

impl From<f64> for Tari {
    fn from(v: f64) -> Self {
        Tari(v)
    }
}

impl From<MicroTari> for Tari {
    fn from(v: MicroTari) -> Self {
        Tari(v.0 as f64 * 1e-6)
    }
}

#[cfg(test)]
mod test {
    use super::{MicroTari, Tari};
    use std::str::FromStr;
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
        let tari = Tari::from(1.12);
        let s = format!("{}", tari.formatted());
        assert_eq!(MicroTari::from(tari), MicroTari::from_str(s.as_str()).unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5000000").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5,000,000").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5,000,000 uT").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5000000 uT").unwrap());
        assert_eq!(MicroTari::from(5_000_000), MicroTari::from_str("5 T").unwrap());
        assert!(MicroTari::from_str("-5 T").is_err());
        assert!(MicroTari::from_str("-5 uT").is_err());
        assert!(MicroTari::from_str("5garbage T").is_err());
    }

    #[test]
    fn add_tari_and_microtari() {
        let a = MicroTari::from(100_000);
        let b = Tari::from(0.23);
        let sum: Tari = b + a.into();
        assert_eq!(sum, Tari::from(0.33));
    }

    #[test]
    fn tari_arithmetic() {
        let mut a = Tari::from(1.5);
        let b = Tari::from(2.25);
        assert_eq!(a + b, Tari::from(3.75));
        assert_eq!(a - b, Tari::from(-0.75));
        assert_eq!(a * 10.0, Tari::from(15.0));
        assert_eq!(b / 2.0, Tari::from(1.125));
        a += b;
        assert_eq!(a, Tari::from(3.75));
        a -= Tari::from(0.75);
        assert_eq!(a, Tari::from(3.0));
    }

    #[test]
    fn tari_display() {
        let s = format!("{}", Tari::from(1.234));
        assert_eq!(s, "1.234000 T");
        let s = format!("{}", Tari::from(99.100));
        assert_eq!(s, "99.100000 T");
    }

    #[test]
    fn formatted_tari_display() {
        let s = format!("{}", Tari::from(1.234).formatted());
        assert_eq!(s, "1.23 T");
        let s = format!("{}", Tari::from(99999.100).formatted());
        assert_eq!(s, "99,999.10 T");
    }
}
