use thiserror::Error;

/// Enum to show failure information
#[derive(Debug, Clone, Error)]
pub enum ExitCodes {
    #[error("There is an error in the configuration: {0}")]
    ConfigError(String),
    #[error("The application exited because an unknown error occurred: {0}. Check the logs for more details.")]
    UnknownError(String),
    #[error("The application exited because an interface error occurred. Check the logs for details.")]
    InterfaceError,
    #[error("The application exited. {0}")]
    WalletError(String),
    #[error("The wallet was not able to start the GRPC server. {0}")]
    GrpcError(String),
    #[error("The application did not accept the command input: {0}")]
    InputError(String),
    #[error("Invalid command: {0}")]
    CommandError(String),
    #[error("IO error: {0}")]
    IOError(String),
    #[error("Recovery failed: {0}")]
    RecoveryError(String),
    #[error("The wallet exited because of an internal network error: {0}")]
    NetworkError(String),
    #[error("The wallet exited because it received a message it could not interpret: {0}")]
    ConversionError(String),
    #[error("Your password was incorrect.")]
    IncorrectPassword,
    #[error("Your application is encrypted but no password was provided.")]
    NoPassword,
    #[error("The application encountered a database error: {0}")]
    DatabaseError(String),
    #[error("Tor connection is offline")]
    TorOffline,
    #[error("Database is in inconsistent state: {0}")]
    DbInconsistentState(String),
}

impl ExitCodes {
    pub fn as_i32(&self) -> i32 {
        match self {
            Self::ConfigError(_) => 101,
            Self::UnknownError(_) => 102,
            Self::InterfaceError => 103,
            Self::WalletError(_) => 104,
            Self::GrpcError(_) => 105,
            Self::InputError(_) => 106,
            Self::CommandError(_) => 107,
            Self::IOError(_) => 108,
            Self::RecoveryError(_) => 109,
            Self::NetworkError(_) => 110,
            Self::ConversionError(_) => 111,
            Self::IncorrectPassword | Self::NoPassword => 112,
            Self::TorOffline => 113,
            Self::DatabaseError(_) => 114,
            Self::DbInconsistentState(_) => 115,
        }
    }

    pub fn eprint_details(&self) {
        use ExitCodes::*;
        match self {
            TorOffline => {
                eprintln!("Unable to connect to the Tor control port.");
                eprintln!(
                    "Please check that you have the Tor proxy running and that access to the Tor control port is \
                     turned on.",
                );
                eprintln!("If you are unsure of what to do, use the following command to start the Tor proxy:");
                eprintln!(
                    "tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport \
                     127.0.0.1:9051 --log \"warn stdout\" --clientuseipv6 1",
                );
            },

            e => {
                eprintln!("{}", e);
            },
        }
    }
}

impl From<super::ConfigError> for ExitCodes {
    fn from(err: super::ConfigError) -> Self {
        // TODO: Move it out
        // error!(target: LOG_TARGET, "{}", err);
        Self::ConfigError(err.to_string())
    }
}

impl From<crate::ConfigurationError> for ExitCodes {
    fn from(err: crate::ConfigurationError) -> Self {
        Self::ConfigError(err.to_string())
    }
}

impl ExitCodes {
    pub fn grpc<M: std::fmt::Display>(err: M) -> Self {
        ExitCodes::GrpcError(format!("GRPC connection error: {}", err))
    }
}
