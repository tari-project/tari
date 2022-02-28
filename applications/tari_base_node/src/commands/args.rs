use std::{
    iter::Peekable,
    str::{FromStr, SplitWhitespace},
};

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
    #[error("argument required")]
    Required,
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

pub struct Args<'a> {
    splitted: Peekable<SplitWhitespace<'a>>,
}

impl<'a> Args<'a> {
    pub fn split(s: &'a str) -> Self {
        Self {
            splitted: s.split_whitespace().peekable(),
        }
    }

    fn shift(&mut self) {
        self.splitted.next();
    }

    /// Try parse the next argument into T. If the parse succeeds, the argument is consumed.
    pub fn try_take_next<T>(&mut self) -> Result<Option<T>, ArgsError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        match self.splitted.peek().and_then(|s| s.parse().ok()) {
            Some(value) => {
                // Value parse succeeded, shift the arg
                self.shift();
                Ok(Some(value))
            },
            None => Ok(None),
        }
    }

    pub fn take_next<T>(&mut self, name: &'static str) -> Result<T, ArgsError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        self.try_take_next()?
            .ok_or_else(|| ArgsError::new(name, ArgsReason::Required))
    }
}

pub struct FromHex<T>(pub T);

impl<T: Hex> FromStr for FromHex<T> {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_hex(s).map(Self)
    }
}
