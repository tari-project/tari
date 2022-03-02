use std::str::FromStr;

use tari_utilities::hex::{Hex, HexError};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{name} {reason}")]
pub struct ArgsError {
    name: &'static str,
    reason: ArgsReason,
}

impl ArgsError {
    pub fn new(name: &'static str, reason: impl Into<ArgsReason>) -> Self {
        Self {
            name,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ArgsReason {
    #[error("argument can't be parsed: {details}")]
    NotParsed { details: String },
    #[error("argument is not valid: {description}")]
    Inconsistent { description: String },
}

impl<T: AsRef<str>> From<T> for ArgsReason {
    fn from(value: T) -> Self {
        Self::Inconsistent {
            description: value.as_ref().to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct FromHex<T>(pub T);

impl<T: Hex> FromStr for FromHex<T> {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_hex(s).map(Self)
    }
}
