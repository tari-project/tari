// Copyright 2020. The Tari Project
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

use std::{fs, fs::File, io, io::Write, path::Path, sync::Arc};

use log::*;
use serde::{de::DeserializeOwned, Serialize};
use tari_common::{
    configuration::bootstrap::prompt,
    exit_codes::{ExitCode, ExitError},
};
use tari_network::identity;

pub const LOG_TARGET: &str = "minotari_application";

const REQUIRED_IDENTITY_PERMS: u32 = 0o100600;

/// Loads the node identity, or creates a new one if create_id is true
///
/// ## Parameters
/// - `identity_file` - Reference to file path
/// - `public_address` - Network address of the base node
/// - `create_id` - Only applies if the identity_file does not exist or is malformed. If true, a new identity will be
///   created, otherwise the user will be prompted to create a new ID
/// - `peer_features` - Enables features of the base node
///
/// # Return
/// A NodeIdentity wrapped in an atomic reference counter on success, the exit code indicating the reason on failure
pub fn setup_node_identity<P: AsRef<Path>>(
    identity_file: P,
    create_id: bool,
) -> Result<Arc<identity::Keypair>, ExitError> {
    match load_key_pair(&identity_file) {
        Ok(id) => Ok(Arc::new(id)),
        Err(IdentityError::InvalidPermissions) => Err(ExitError::new(
            ExitCode::ConfigError,
            format!(
                "{path} has incorrect permissions. You can update the identity file with the correct permissions \
                 using 'chmod 600 {path}', or delete the identity file and a new one will be created on next start",
                path = identity_file.as_ref().to_string_lossy()
            ),
        )),
        Err(e) => {
            debug!(target: LOG_TARGET, "Failed to load node identity: {}", e);
            if !create_id {
                let prompt = prompt("Node identity does not exist.\nWould you like to create one (Y/n)?");
                if !prompt {
                    error!(
                        target: LOG_TARGET,
                        "Node identity not found. {}. You can update the configuration file to point to a valid node \
                         identity file, or re-run the node and create a new one.",
                        e
                    );
                    return Err(ExitError::new(
                        ExitCode::ConfigError,
                        format!(
                            "Node identity information not found. {}. You can update the configuration file to point \
                             to a valid node identity file, or re-run the node to create a new one",
                            e
                        ),
                    ));
                };
            }

            debug!(target: LOG_TARGET, "Existing node id not found. {}. Creating new ID", e);

            let id = identity::Keypair::generate_sr25519();
            save_identity(identity_file.as_ref(), &id).map_err(|e| ExitError::new(ExitCode::IdentityError, e))?;
            info!(
                target: LOG_TARGET,
                "New node identity [{}] has been created at {}.",
                id.public().to_peer_id(),
                identity_file.as_ref().to_str().unwrap_or("?"),
            );
            Ok(Arc::new(id))
        },
    }
}

/// Tries to construct a node identity by loading the secret key and other metadata from disk and calculating the
/// missing fields from that information.
///
/// ## Parameters
/// `path` - Reference to a path
///
/// ## Returns
/// Result containing a NodeIdentity on success, string indicates the reason on failure
fn load_key_pair<P: AsRef<Path>>(path: P) -> Result<identity::Keypair, IdentityError> {
    check_identity_file(&path)?;

    let bytes = fs::read(path.as_ref())?;
    let id = identity::Keypair::from_protobuf_encoding(&bytes)?;
    debug!("Keypair {} loaded", id.public().to_peer_id(),);
    Ok(id)
}

/// Loads the node identity from json at the given path
///
/// ## Parameters
/// `path` - Path to file from which to load the node identity
///
/// ## Returns
/// Result containing an object on success, string will indicate reason on error
pub fn load_from_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<Option<T>, IdentityError> {
    if !path.as_ref().exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path)?;
    let object = json5::from_str(&contents)?;
    Ok(Some(object))
}

/// Saves the identity as json at a given path with 0600 file permissions (UNIX-only), creating it if it does not
/// already exist.
///
/// ## Parameters
/// `path` - Path to save the file
/// `object` - Data to be saved
///
/// ## Returns
/// Result to check if successful or not, string will indicate reason on error
pub fn save_as_json<P: AsRef<Path>, T: Serialize>(path: P, object: &T) -> Result<(), IdentityError> {
    let json = json5::to_string(object)?;
    if let Some(p) = path.as_ref().parent() {
        if !p.exists() {
            fs::create_dir_all(p)?;
        }
    }
    let json_with_comment = format!(
        "// This file is generated by the Minotari base node. Any changes will be overwritten.\n{}",
        json
    );
    fs::write(path.as_ref(), json_with_comment.as_bytes())?;
    set_permissions(path, REQUIRED_IDENTITY_PERMS)?;
    Ok(())
}

/// Writes bytes to the provided file.
fn write_bytes_to_file<P: AsRef<Path>>(path: P, bytes: &[u8]) -> Result<(), IdentityError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    Ok(())
}

pub fn save_identity<P: AsRef<Path>>(path: P, identity: &identity::Keypair) -> Result<(), IdentityError> {
    write_bytes_to_file(path.as_ref(), &identity.to_protobuf_encoding()?)?;
    set_permissions(path, REQUIRED_IDENTITY_PERMS)?;
    Ok(())
}

/// Check that the given path exists, is a file and has the correct file permissions (mac/linux only)
fn check_identity_file<P: AsRef<Path>>(path: P) -> Result<(), IdentityError> {
    if !path.as_ref().exists() {
        return Err(IdentityError::NotFound);
    }

    if !path.as_ref().metadata()?.is_file() {
        return Err(IdentityError::NotFile);
    }

    if !has_permissions(&path, REQUIRED_IDENTITY_PERMS)? {
        return Err(IdentityError::InvalidPermissions);
    }
    Ok(())
}

#[cfg(target_family = "unix")]
fn set_permissions<P: AsRef<Path>>(path: P, new_perms: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(&path)?;
    let mut perms = metadata.permissions();
    perms.set_mode(new_perms);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(target_family = "windows")]
fn set_permissions<P: AsRef<Path>>(_: P, _: u32) -> io::Result<()> {
    // Windows permissions are very different and are not supported
    Ok(())
}

#[cfg(target_family = "unix")]
fn has_permissions<P: AsRef<Path>>(path: P, perms: u32) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    Ok(metadata.permissions().mode() == perms)
}

#[cfg(target_family = "windows")]
fn has_permissions<P: AsRef<Path>>(_: P, _: u32) -> io::Result<bool> {
    Ok(true)
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("Identity file has invalid permissions")]
    InvalidPermissions,
    #[error("Identity file was not found")]
    NotFound,
    #[error("Path is not a file")]
    NotFile,
    #[error("Malformed identity file: {0}")]
    JsonError(#[from] json5::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Decoding error: {0}")]
    DecodingError(#[from] identity::DecodingError),
}
