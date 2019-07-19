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
use std::fmt::{Display, Error, Formatter};

use std::{iter::Sum, ops::Add};
use tari_crypto::ristretto::RistrettoSecretKey;

/// All calculations using Tari amounts should use these newtypes to prevent bugs related to rounding errors, unit
/// conversion errors etc.
///
/// ```edition2018
/// use tari_core::tari_amount::MicroTari;
///
/// let a = MicroTari::from(500);
/// let b = MicroTari::from(50);
/// assert_eq!(a + b, MicroTari::from(550));
/// ```
#[derive(Copy, Default, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MicroTari(pub u64);

// You can only add or subtract µT from µT
newtype_ops! { [MicroTari] {add sub} {:=} Self Self }
newtype_ops! { [MicroTari] {add sub} {:=} &Self &Self }
newtype_ops! { [MicroTari] {add sub} {:=} Self &Self }

// Multiplication and division only makes sense when µT is multiplied/divided by a scalar
newtype_ops! { [MicroTari] {mul div rem} {:=} Self u64 }

impl MicroTari {
    pub fn checked_sub(self, v: MicroTari) -> Option<MicroTari> {
        if self.0 >= v.0 {
            return Some(self - v);
        }
        None
    }
}

impl Display for MicroTari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_fmt(format_args!("{} µT", self.0))
    }
}

impl From<MicroTari> for u64 {
    fn from(v: MicroTari) -> Self {
        v.0
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

/// A convenience struct for representing full Tari. You should **never** use Tari in consensus calculations, because
/// Tari wraps a floating point value. Use MicroTari for that instead.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct Tari(f64);

newtype_ops! { [Tari] {add sub} {:=} Self Self }
newtype_ops! { [Tari] {mul div rem} {:=} Self f64 }

impl Display for Tari {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_fmt(format_args!("{:0.6} T", self.0))
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
    use crate::tari_amount::{MicroTari, Tari};

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
    }
}
