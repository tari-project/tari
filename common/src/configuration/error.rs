// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt;

use structopt::clap::Error as ClapError;

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
            write!(f, ": {}", source)?
        }

        Ok(())
    }
}

impl From<ClapError> for ConfigError {
    fn from(e: ClapError) -> Self {
        Self {
            cause: "Failed to process commandline parameters",
            source: Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod test {
    use structopt::clap::{Error as ClapError, ErrorKind};

    use super::*;

    #[test]
    fn config_error_test() {
        // new config error
        let config_error = ConfigError::new("testing", Some(String::from("coverage")));

        // test formatting
        assert_eq!(format!("{}", config_error), "testing: coverage");

        // create new error
        let clap_error = ClapError {
            message: String::from("error"),
            kind: ErrorKind::InvalidValue,
            info: Some(vec![String::from("for test purposes")]),
        };
        // get clap error string
        let clap_error_str = clap_error.to_string();
        // create a new config error from clap error
        let new_config_error = ConfigError::from(clap_error);

        // test specification of config error from clap error
        assert_eq!(new_config_error.cause, "Failed to process commandline parameters");
        assert_eq!(new_config_error.source, Some(clap_error_str));
    }
}
