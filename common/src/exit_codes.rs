use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub struct ExitError {
    pub exit_code: ExitCode,
    pub details: Option<String>,
}

impl ExitError {
    pub fn new(exit_code: ExitCode, details: &impl ToString) -> Self {
        let details = Some(details.to_string());
        Self { exit_code, details }
    }
}

impl From<ExitCode> for ExitError {
    fn from(exit_code: ExitCode) -> Self {
        Self {
            exit_code,
            details: None,
        }
    }
}

impl fmt::Display for ExitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let details = self.details.as_ref().map(String::as_ref).unwrap_or("");
        write!(f, "{} {}", self.exit_code, details)
    }
}

const TOR_HINT: &str = r#"\
Unable to connect to the Tor control port.

Please check that you have the Tor proxy running and \
that access to the Tor control port is turned on.

If you are unsure of what to do, use the following \
command to start the Tor proxy:
tor --allow-missing-torrc --ignore-missing-torrc \
--clientonly 1 --socksport 9050 --controlport \
127.0.0.1:9051 --log \"warn stdout\" --clientuseipv6 1
"#;

impl ExitCode {
    pub fn hint(&self) -> &str {
        use ExitCode::TorOffline;
        match self {
            TorOffline => TOR_HINT,
            _ => "",
        }
    }
}

/// Enum to show failure information
#[derive(Debug, Clone, Copy, Error)]
pub enum ExitCode {
    #[error("There is an error in the configuration.")]
    ConfigError = 101,
    #[error("The application exited because an unknown error occurred. Check the logs for more details.")]
    UnknownError = 102,
    #[error("The application exited because an interface error occurred. Check the logs for details.")]
    InterfaceError = 103,
    #[error("The application exited.")]
    WalletError = 104,
    #[error("The application was not able to start the GRPC server.")]
    GrpcError = 105,
    #[error("The application did not accept the command input.")]
    InputError = 106,
    #[error("Invalid command.")]
    CommandError = 107,
    #[error("IO error.")]
    IOError = 108,
    #[error("Recovery failed.")]
    RecoveryError = 109,
    #[error("The application exited because of an internal network error.")]
    NetworkError = 110,
    #[error("The application exited because it received a message it could not interpret.")]
    ConversionError = 111,
    #[error("Your password was incorrect or required, but not provided.")]
    IncorrectOrEmptyPassword = 112,
    #[error("Tor connection is offline")]
    TorOffline = 113,
    #[error("The application encountered a database error.")]
    DatabaseError = 114,
    #[error("Database is in an inconsistent state!")]
    DbInconsistentState = 115,
}

impl From<super::ConfigError> for ExitError {
    fn from(err: super::ConfigError) -> Self {
        // TODO: Move it out
        // error!(target: LOG_TARGET, "{}", err);
        Self::new(ExitCode::ConfigError, &err)
    }
}

impl From<crate::ConfigurationError> for ExitError {
    fn from(err: crate::ConfigurationError) -> Self {
        Self::new(ExitCode::ConfigError, &err)
    }
}

impl From<multiaddr::Error> for ExitError {
    fn from(err: multiaddr::Error) -> Self {
        Self::new(ExitCode::ConfigError, &err)
    }
}

impl From<std::io::Error> for ExitError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ExitCode::IOError, &err)
    }
}
