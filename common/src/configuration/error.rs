use std::fmt;
use structopt::clap::Error as ClapError;

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
