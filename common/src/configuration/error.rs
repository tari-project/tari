// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt;


use crate::network_check::NetworkCheckError;

#[derive(Debug)]
pub struct ConfigError {
    pub(crate) cause: &'static str,
    pub(crate) source: Option<String>,
}

impl ConfigError {
    pub(crate) fn new(cause: &'static str, source: Option<String>) -> Self {
        Self { cause, source }
    }
}

impl From<NetworkCheckError> for ConfigError {
    fn from(err: NetworkCheckError) -> Self {
        Self {
            cause: "Failed to set the network",
            source: Some(err.to_string()),
        }
    }
}

impl std::error::Error for ConfigError {}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.cause)?;
        if let Some(ref source) = self.source {
            write!(f, ": {source}")?
        }

        Ok(())
    }
}