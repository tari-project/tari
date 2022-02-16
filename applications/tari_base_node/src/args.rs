use std::{
    iter::Peekable,
    str::{FromStr, SplitWhitespace},
};

use tari_app_utilities::utilities::{either_to_node_id, parse_emoji_id_or_public_key_or_node_id};
use tari_comms::peer_manager::NodeId;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{name} {reason}")]
pub struct ArgsError {
    name: &'static str,
    reason: ArgsReason,
}

impl ArgsError {
    pub fn new(name: &'static str, reason: ArgsReason) -> Self {
        Self { name, reason }
    }
}

#[derive(Debug, Error)]
pub enum ArgsReason {
    #[error("argument required")]
    Required,
    #[error("argument can't be parsed: {details}")]
    NotParsed { details: String },
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

    pub fn shift_one(&mut self) {
        self.splitted.next();
    }

    // TODO: It have to return error if a value provided,
    // but can''t be parsed
    pub fn take_node_id(&mut self) -> Option<NodeId> {
        self.splitted
            .next()
            .and_then(parse_emoji_id_or_public_key_or_node_id)
            .map(either_to_node_id)
    }

    pub fn try_take_next<T>(&mut self, name: &'static str) -> Result<Option<T>, ArgsError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        match self.splitted.peek().map(|s| s.parse()) {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(err)) => Err(ArgsError::new(name, ArgsReason::NotParsed {
                details: err.to_string(),
            })),
            None => Ok(None),
        }
    }

    pub fn take_next<T>(&mut self, name: &'static str) -> Result<T, ArgsError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        self.try_take_next(name)?
            .ok_or_else(|| ArgsError::new(name, ArgsReason::Required))
    }
}
