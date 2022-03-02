use std::str::FromStr;

use tari_utilities::hex::{Hex, HexError};

#[derive(Debug)]
pub struct FromHex<T>(pub T);

impl<T: Hex> FromStr for FromHex<T> {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_hex(s).map(Self)
    }
}
