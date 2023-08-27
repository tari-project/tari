// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub struct ExitError {
    pub exit_code: ExitCode,
    pub details: Option<String>,
}

impl ExitError {
    pub fn new<T: ToString>(exit_code: ExitCode, details: T) -> Self {
        Self {
            exit_code,
            details: Some(details.to_string()),
        }
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

impl From<anyhow::Error> for ExitError {
    fn from(err: anyhow::Error) -> Self {
        ExitError::new(ExitCode::UnknownError, err)
    }
}

const TOR_HINT: &str = r#"Unable to connect to the Tor control port.

Please check that you have the Tor proxy running and that access to the Tor control port is turned on in your `torrc`.

If you are unsure of what to do, use the following command to start the Tor proxy:
tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --log "warn stdout" --clientuseipv6 1"#;

const TOR_CONFIG_AUTH_HINT: &str = r#"Unable to authenticate to the Tor control port.

Please check the Tor control port configuration in your torrc and update your Taiji configuration to match the configured authentication method.

If you are unsure of what to do, use the following command to start the Tor proxy:
tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --log "warn stdout" --clientuseipv6 1 --cookieauthentication 1"#;

const TOR_AUTH_UNREADABLE_COOKIE_HINT: &str = r#"Unable to read tor control port cookie file.

The current user must have permissions to read the tor control port cookie file. 

On a linux system this means adding your current user to the `debian-tor` group with the following command (requires root):
sudo usermod -aG debian-tor $USER

If you are unsure of what to do, use the following command to start the Tor proxy:
tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --log "warn stdout" --clientuseipv6 1 --cookieauthentication 1"#;

impl ExitCode {
    pub fn hint(&self) -> Option<&str> {
        #[allow(clippy::enum_glob_use)]
        use ExitCode::*;
        match self {
            TorOffline => Some(TOR_HINT),
            TorAuthConfiguration => Some(TOR_CONFIG_AUTH_HINT),
            TorAuthUnreadableCookie => Some(TOR_AUTH_UNREADABLE_COOKIE_HINT),
            _ => None,
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
    #[error("DigitalAssetError")]
    DigitalAssetError = 116,
    #[error("Unable to create or load an identity file")]
    IdentityError = 117,
    #[error("Tor control port authentication is not configured correctly")]
    TorAuthConfiguration = 118,
    #[error("Unable to read Tor cookie file")]
    TorAuthUnreadableCookie = 119,
}

impl From<super::ConfigError> for ExitError {
    fn from(err: super::ConfigError) -> Self {
        Self::new(ExitCode::ConfigError, err.to_string())
    }
}

impl From<crate::ConfigurationError> for ExitError {
    fn from(err: crate::ConfigurationError) -> Self {
        Self::new(ExitCode::ConfigError, err.to_string())
    }
}

impl From<multiaddr::Error> for ExitError {
    fn from(err: multiaddr::Error) -> Self {
        Self::new(ExitCode::ConfigError, err.to_string())
    }
}

impl From<std::io::Error> for ExitError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ExitCode::IOError, err.to_string())
    }
}
