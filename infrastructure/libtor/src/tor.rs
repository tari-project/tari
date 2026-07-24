// Copyright 2021. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fmt,
    fs,
    io,
    net::TcpListener,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    thread,
};

use derivative::Derivative;
use libtor::{LogDestination, LogLevel, TorFlag};
use log::*;
use rand::{RngExt, distr::Alphanumeric};
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_p2p::{TorControlAuthentication, TransportConfig};
use tor_hash_passwd::EncryptedKey;
use zeroize::Zeroizing;

const LOG_TARGET: &str = "tari_libtor";

pub struct TorPassword(Option<Zeroizing<String>>);

impl fmt::Debug for TorPassword {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TorPassword: ...")
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Tor {
    control_port: u16,
    data_dir: PathBuf,
    log_destination: PathBuf,
    log_level: LogLevel,
    #[derivative(Debug = "ignore")]
    passphrase: TorPassword,
    socks_port: u16,
}

impl Tor {
    /// Returns a new Tor instance with random options.
    /// The data directory, passphrase, and log destination are temporary and randomized.
    /// The control port is provided by the operating system; the socks port is chosen by Tor itself.
    /// The onion address and port info are still loaded from the node identity file.
    ///
    /// There is deliberately no `Default`/public constructor: a `Tor` can only be created via
    /// `initialize`, which always sets a private, hashed control-port passphrase and an owner-only
    /// data directory. This avoids handing out an instance that starts an unauthenticated control
    /// port over a world-writable `/tmp` directory holding onion-service private keys.
    pub fn initialize(base_dir: PathBuf) -> Result<Tor, ExitError> {
        debug!(target: LOG_TARGET, "Initializing libtor");

        // The control port must be known up-front because `update_comms_transport` wires it into the
        // comms transport before Tor is started. The socks port has no such requirement, so it is left
        // to Tor's `SocksPortAuto` rather than being probed here.
        let control_port = get_available_port()?;
        debug!(target: LOG_TARGET, "Using auto socks port and control_port {control_port}");

        // generate a random control-port passphrase
        let passphrase: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(30)
            .map(char::from)
            .collect();

        // The data directory holds onion-service private keys, so it must be owner-only (0700).
        let data_dir = base_dir.join("data");
        create_secure_dir(&data_dir)?;

        let instance = Tor {
            control_port,
            data_dir,
            log_destination: base_dir.join("tor.log"),
            log_level: LogLevel::Err,
            passphrase: TorPassword(Some(Zeroizing::new(passphrase))),
            socks_port: 0,
        };

        debug!(target: LOG_TARGET, "tor instance: {instance:?}");
        Ok(instance)
    }

    /// Override a given Tor comms transport with the control address and auth from this instance
    pub fn update_comms_transport(&self, transport: &mut TransportConfig) -> Result<(), ExitError> {
        if !transport.transport_type.uses_tor_hidden_service() {
            let e = format!("Expected a TorHiddenService comms transport, received: {transport:?}");
            return Err(ExitError::new(ExitCode::ConfigError, e));
        }

        if let Some(ref passphrase) = self.passphrase.0 {
            transport.tor.control_auth = TorControlAuthentication::Password(passphrase.as_str().to_owned());
        }
        transport.tor.control_address = format!("/ip4/127.0.0.1/tcp/{}", self.control_port)
            .parse()
            .map_err(|e| {
                ExitError::new(
                    ExitCode::ConfigError,
                    format!(
                        "Failed to construct Tor control address for port {}: {}",
                        self.control_port, e
                    ),
                )
            })?;
        debug!(target: LOG_TARGET, "updated comms transport: {transport:?}");
        Ok(())
    }

    /// Run the Tor instance in the background and return a handle to the thread.
    pub fn run_background(self) -> thread::JoinHandle<Result<u8, libtor::Error>> {
        info!(target: LOG_TARGET, "Starting Tor instance");

        let Tor {
            data_dir,
            socks_port,
            control_port,
            log_level,
            log_destination,
            passphrase,
            ..
        } = self;

        let mut tor = libtor::Tor::new();

        tor.flag(TorFlag::DataDirectory(data_dir.to_string_lossy().to_string()))
            // Disable signal handlers so that ctrl+c can be handled by our application
            // https://github.com/torproject/torspec/blob/8961bb4d83fccb2b987f9899ca83aa430f84ab0c/control-spec.txt#L3946
            .flag(TorFlag::Custom("__DisableSignalHandlers 1".to_string()))
            // Prevent conflicts with multiple instances using the same listener port for Prometheus metrics
            .flag(TorFlag::Custom("MetricsPort 0".to_string()))
            // Write the final control port to a file. This could be used to configure the node to use this port when auto is set.
            .flag(TorFlag::ControlPortWriteToFile(data_dir.join("control_port").to_string_lossy().to_string()))
            .flag(TorFlag::Hush())
            .flag(TorFlag::LogTo(log_level, LogDestination::File(log_destination.to_string_lossy().to_string())));

        if socks_port == 0 {
            tor.flag(TorFlag::SocksPortAuto);
        } else {
            tor.flag(TorFlag::SocksPort(socks_port));
        }

        if control_port == 0 {
            tor.flag(TorFlag::ControlPortAuto);
        } else {
            tor.flag(TorFlag::ControlPort(control_port));
        }

        if let Some(secret) = passphrase.0 {
            let hash = EncryptedKey::hash_password(secret.as_str()).to_string();
            tor.flag(TorFlag::HashedControlPassword(hash));
            // `secret` (Zeroizing) is wiped from memory as it drops here.
        }

        tor.start_background()
    }
}

/// Create `path` (and any missing parents) and ensure it is only accessible by the owner (0700).
///
/// The Tor data directory stores onion-service private keys, so it must never be group- or
/// world-readable. `DirBuilder::mode` sets the permissions on the directories it creates, and the
/// explicit `set_permissions` afterwards also tightens an already-existing directory (where
/// `create` is a no-op) instead of relying on the process umask.
fn create_secure_dir(path: &Path) -> Result<(), ExitError> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|e| {
            ExitError::new(
                ExitCode::InputError,
                format!("Could not create libtor data directory: {} ({})", path.display(), e),
            )
        })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| {
        ExitError::new(
            ExitCode::InputError,
            format!("Could not secure libtor data directory: {} ({})", path.display(), e),
        )
    })
}

/// Attempt to find an available TCP port for the Tor control port.
///
/// Only the control port is probed: it must be known before Tor starts so it can be written into
/// the comms transport by [`Tor::update_comms_transport`]. The socks port is instead left to Tor's
/// `SocksPortAuto`. A probe like this has an inherent TOCTOU window between closing the listener and
/// Tor re-binding the port; it is unavoidable while the control address must be known ahead of
/// Tor start-up, but binding to `127.0.0.1` keeps the exposure local to the host.
fn get_available_port() -> Result<u16, io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}
