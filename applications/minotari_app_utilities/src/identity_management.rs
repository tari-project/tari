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

use std::{
    fs,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use log::*;
use serde::{Serialize, de::DeserializeOwned};
use tari_common::{
    configuration::bootstrap::prompt_default_yes_on_eof,
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{
    NodeIdentity,
    multiaddr::{Multiaddr, Protocol},
    peer_manager::PeerFeatures,
    tor::TorIdentity,
};
use tari_p2p::TransportType;
use tari_utilities::hex::Hex;

pub const LOG_TARGET: &str = "minotari_application";

#[cfg(target_family = "unix")]
const CREATED_IDENTITY_FILE_MODE: u32 = 0o600;
#[cfg(target_family = "unix")]
const CREATED_IDENTITY_DIR_MODE: u32 = 0o700;
#[cfg(target_family = "unix")]
const GROUP_OR_OTHER_MODE_MASK: u32 = 0o077;
#[cfg(target_family = "unix")]
const OWNER_READ_MODE_MASK: u32 = 0o400;

#[cfg(target_family = "windows")]
const WINDOWS_PERMISSION_WARNING: &str = "Identity-file permissions are not enforced on this platform; ensure the \
                                          identity directory is restricted to the current user";

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
    public_addresses: Vec<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
    transport_type: TransportType,
) -> Result<Arc<NodeIdentity>, ExitError> {
    match load_node_identity(&identity_file, transport_type) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            // Filter addresses based on transport type
            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            for public_address in filtered_addresses {
                id.add_public_address(public_address.clone());
                debug!(
                    target: LOG_TARGET,
                    "Added address: {}",
                    public_address
                );
            }
            Ok(Arc::new(id))
        },
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            identity_file.as_ref(),
            "a new one will be created on next start",
        )),
        Err(
            e @ (IdentityError::Symlink |
            IdentityError::HardLink { .. } |
            IdentityError::InvalidOwner { .. } |
            IdentityError::NotFile |
            IdentityError::UnsafeDirectory { .. }),
        ) => Err(ExitError::new(
            ExitCode::ConfigError,
            format!(
                "Identity file {} failed security check: {e}",
                identity_file.as_ref().to_string_lossy()
            ),
        )),
        Err(e) => {
            warn!(target: LOG_TARGET, "Failed to load node identity: {e}");
            if !create_id {
                let prompt =
                    prompt_default_yes_on_eof("Node identity does not exist.\nWould you like to create one (Y/n)?");
                if !prompt {
                    error!(
                        target: LOG_TARGET,
                        "Node identity not found. {e}. You can update the configuration file to point to a valid node \
                         identity file, or re-run the node and create a new one."
                    );
                    return Err(ExitError::new(
                        ExitCode::ConfigError,
                        format!(
                            "Node identity information not found. {e}. You can update the configuration file to point \
                             to a valid node identity file, or re-run the node to create a new one"
                        ),
                    ));
                };
            }
            debug!(target: LOG_TARGET, "Existing node id not found. {e}. Creating new ID");

            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            match create_new_node_identity(&identity_file, filtered_addresses, peer_features) {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "New node identity [{}] with public key {} has been created at {}.",
                        id.node_id(),
                        id.public_key(),
                        identity_file.as_ref().to_str().unwrap_or("?"),
                    );
                    Ok(Arc::new(id))
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not create new node id. {e}.");
                    Err(ExitError::new(
                        ExitCode::ConfigError,
                        format!("Could not create new node id. {e}."),
                    ))
                },
            }
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
fn load_node_identity<P: AsRef<Path>>(path: P, transport_type: TransportType) -> Result<NodeIdentity, IdentityError> {
    let id_str = read_identity_file(path.as_ref())?;
    let id = json5::from_str::<NodeIdentity>(&id_str)?;
    let mut needs_save = false;

    let id = if transport_type == TransportType::Tcp {
        let current_addresses = id.public_addresses();
        debug!(
            target: LOG_TARGET,
            "Filtering addresses for TCP transport. Current addresses: {:?}",
            current_addresses
        );
        // For TCP transport remove all onion addresses
        let filtered_addresses: Vec<Multiaddr> = current_addresses
            .iter()
            .filter(|addr| !addr.iter().any(|p| matches!(p, Protocol::Onion3(_))))
            .cloned()
            .collect();
        debug!(
            target: LOG_TARGET,
            "After filtering for TCP transport, {} addresses remain: {:?}",
            filtered_addresses.len(),
            filtered_addresses
        );
        if filtered_addresses != current_addresses {
            id.set_public_addresses(filtered_addresses);
            needs_save = true;
        }
        id
    } else {
        id
    };

    // Check whether the previous version has a signature and sign if necessary
    if !id.is_signed() {
        id.sign();
        needs_save = true;
    }
    debug!(
        target: LOG_TARGET,
        "Node ID loaded with public key {} and Node id {}",
        id.public_key().to_hex(),
        id.node_id().to_hex()
    );
    if needs_save {
        save_as_json(&path, &id)?;
    }
    Ok(id)
}

/// Filter addresses based on transport type
fn filter_addresses_for_transport(addresses: Vec<Multiaddr>, transport_type: TransportType) -> Vec<Multiaddr> {
    if transport_type == TransportType::Tcp {
        // Filter out onion addresses for TCP transport
        let filtered: Vec<Multiaddr> = addresses
            .into_iter()
            .filter(|addr| !addr.iter().any(|p| matches!(p, Protocol::Onion3(_))))
            .collect();
        debug!(
            target: LOG_TARGET,
            "Filtered addresses for TCP transport: {:?}",
            filtered
        );
        filtered
    } else {
        debug!(
            target: LOG_TARGET,
            "No filtering for {:?} transport, keeping all {} addresses",
            transport_type,
            addresses.len()
        );
        addresses
    }
}

/// Create a new node id and save it to disk
///
/// ## Parameters
/// `path` - Reference to path to save the file
/// `public_addr` - Network address of the base node
/// `peer_features` - The features enabled for the base node
///
/// ## Returns
/// Result containing the node identity, string will indicate reason on error
fn create_new_node_identity<P: AsRef<Path>>(
    path: P,
    public_addresses: Vec<Multiaddr>,
    features: PeerFeatures,
) -> Result<NodeIdentity, IdentityError> {
    let node_identity = NodeIdentity::random_multiple_addresses(&mut rand::rng(), public_addresses, features);
    save_as_json(&path, &node_identity)?;
    Ok(node_identity)
}

/// Loads the node identity from json at the given path
///
/// ## Parameters
/// `path` - Path to file from which to load the node identity
///
/// ## Returns
/// Result containing an object on success, string will indicate reason on error
pub fn load_from_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<Option<T>, IdentityError> {
    let contents = match read_identity_file(path.as_ref()) {
        Ok(contents) => contents,
        Err(IdentityError::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    let object = json5::from_str(&contents)?;
    Ok(Some(object))
}

/// Attempts to load the TorIdentity from the JSON file at the given path.
///
/// ## Parameters
/// `path` - Path to the `TorIdentity` JSON file
///
/// ## Returns
/// The deserialized `TorIdentity` struct. Returns an Ok(None) if the path does not exist, the exit code indicating the
/// reason on failure.
pub fn load_tor_identity<P: AsRef<Path>>(path: P) -> Result<Option<TorIdentity>, ExitError> {
    match load_from_json(&path) {
        Ok(identity) => Ok(identity),
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            path.as_ref(),
            "a new one, with a new onion address, will be created on next start",
        )),
        Err(e) => Err(ExitError::new(
            ExitCode::ConfigError,
            format!(
                "Could not load Tor identity from {}: {e}",
                path.as_ref().to_string_lossy()
            ),
        )),
    }
}

/// The error returned when an identity file is accessible to group or other users, telling the user how to fix it.
/// `recreated` describes what happens if the user deletes the file instead.
fn invalid_permissions_error(path: &Path, recreated: &str) -> ExitError {
    ExitError::new(
        ExitCode::ConfigError,
        format!(
            "{path} must be readable only by its owner; run `chmod 600` or `chmod 400` (e.g. 'chmod 600 {path}'), or \
             delete the identity file and {recreated}",
            path = path.to_string_lossy()
        ),
    )
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
    if let Some(p) = path.as_ref().parent() &&
        !p.as_os_str().is_empty()
    {
        ensure_identity_dir(p)?;
    }
    let json_with_comment =
        format!("// This file is generated by the Minotari base node. Any changes will be overwritten.\n{json}");
    write_identity_file(path.as_ref(), json_with_comment.as_bytes())?;
    Ok(())
}

/// Ensures the parent directory for an identity file exists and has safe ownership and permissions (`0700` on Unix).
#[cfg(target_family = "unix")]
fn ensure_identity_dir(dir: &Path) -> Result<(), IdentityError> {
    use std::{fs::DirBuilder, os::unix::fs::DirBuilderExt};

    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };

    match fs::symlink_metadata(dir) {
        Ok(metadata) => validate_identity_dir_metadata(dir, &metadata, euid),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new()
                .recursive(true)
                .mode(CREATED_IDENTITY_DIR_MODE)
                .create(dir)?;
            let metadata = fs::symlink_metadata(dir)?;
            validate_identity_dir_metadata(dir, &metadata, euid)
        },
        Err(err) => Err(IdentityError::Io(err)),
    }
}

#[cfg(target_family = "unix")]
fn validate_identity_dir_metadata(dir: &Path, metadata: &fs::Metadata, euid: u32) -> Result<(), IdentityError> {
    use std::os::unix::fs::MetadataExt;

    if metadata.file_type().is_symlink() {
        return Err(IdentityError::UnsafeDirectory {
            path: dir.to_path_buf(),
            reason: "is a symbolic link".to_string(),
        });
    }
    if !metadata.is_dir() {
        return Err(IdentityError::UnsafeDirectory {
            path: dir.to_path_buf(),
            reason: "is not a directory".to_string(),
        });
    }
    if metadata.uid() != euid {
        return Err(IdentityError::UnsafeDirectory {
            path: dir.to_path_buf(),
            reason: format!("is owned by uid {}, expected current uid {}", metadata.uid(), euid),
        });
    }
    let mode = metadata.mode() & 0o7777;
    if mode & GROUP_OR_OTHER_MODE_MASK != 0 {
        return Err(IdentityError::UnsafeDirectory {
            path: dir.to_path_buf(),
            reason: format!(
                "has mode {:04o}; must be 0700 (not accessible or writable by group or others)",
                mode
            ),
        });
    }
    Ok(())
}

#[cfg(target_family = "windows")]
fn ensure_identity_dir(dir: &Path) -> Result<(), IdentityError> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(target_family = "unix")]
fn is_nofollow_symlink_error(err: &io::Error) -> bool {
    #[cfg(target_os = "freebsd")]
    if err.raw_os_error() == Some(libc::EMLINK) {
        return true;
    }
    #[cfg(target_os = "netbsd")]
    if err.raw_os_error() == Some(libc::EFTYPE) {
        return true;
    }
    err.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(target_family = "unix")]
fn map_unix_open_error(err: io::Error, path: &Path) -> IdentityError {
    if err.kind() == io::ErrorKind::NotFound {
        return IdentityError::NotFound;
    }
    if is_nofollow_symlink_error(&err) {
        return IdentityError::Symlink;
    }
    if err.raw_os_error() == Some(libc::EISDIR) || err.raw_os_error() == Some(libc::ENXIO) {
        return IdentityError::NotFile;
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return IdentityError::Symlink;
        }
        if !meta.is_file() {
            return IdentityError::NotFile;
        }
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let euid = unsafe { libc::geteuid() };
        if let Err(meta_err) = validate_identity_descriptor_metadata(&meta, euid, true) {
            return meta_err;
        }
    }
    if err.kind() == io::ErrorKind::PermissionDenied {
        return IdentityError::InvalidPermissions;
    }
    IdentityError::Io(err)
}

#[cfg(target_family = "unix")]
fn validate_identity_descriptor_metadata(
    metadata: &fs::Metadata,
    euid: u32,
    check_mode: bool,
) -> Result<(), IdentityError> {
    use std::os::unix::fs::MetadataExt;

    if metadata.file_type().is_symlink() {
        return Err(IdentityError::Symlink);
    }
    if !metadata.is_file() {
        return Err(IdentityError::NotFile);
    }
    if metadata.uid() != euid {
        return Err(IdentityError::InvalidOwner {
            expected_uid: euid,
            actual_uid: metadata.uid(),
        });
    }
    if metadata.nlink() != 1 {
        return Err(IdentityError::HardLink {
            nlink: metadata.nlink(),
        });
    }
    if check_mode &&
        ((metadata.mode() & GROUP_OR_OTHER_MODE_MASK != 0) || (metadata.mode() & OWNER_READ_MODE_MASK == 0))
    {
        return Err(IdentityError::InvalidPermissions);
    }
    Ok(())
}

#[cfg(target_family = "unix")]
fn open_and_validate_identity_file(path: &Path) -> Result<fs::File, IdentityError> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|err| map_unix_open_error(err, path))?;

    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let metadata = file.metadata()?;
    validate_identity_descriptor_metadata(&metadata, euid, true)?;
    Ok(file)
}

#[cfg(target_family = "unix")]
fn read_identity_file(path: &Path) -> Result<String, IdentityError> {
    use std::io::Read;

    let mut file = open_and_validate_identity_file(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(target_family = "windows")]
fn warn_windows_permissions_unforced_once() {
    use std::sync::Once;
    static WARN_ONCE: Once = Once::new();
    WARN_ONCE.call_once(|| {
        warn!(target: LOG_TARGET, "{WINDOWS_PERMISSION_WARNING}");
    });
}

#[cfg(target_family = "windows")]
fn read_identity_file(path: &Path) -> Result<String, IdentityError> {
    check_identity_file(path)?;
    Ok(fs::read_to_string(path)?)
}

/// Writes the identity file. On unix the file is opened with `O_NOFOLLOW | O_NONBLOCK` and validated via `fstat` on
/// the open descriptor (regular file, owned by `geteuid()`, `st_nlink == 1`) before permissions are tightened and
/// contents are truncated and written.
#[cfg(target_family = "unix")]
fn write_identity_file<P: AsRef<Path>>(path: P, contents: &[u8]) -> Result<(), IdentityError> {
    use std::{
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    let path = path.as_ref();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(CREATED_IDENTITY_FILE_MODE)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|err| map_unix_open_error(err, path))?;

    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    let metadata = file.metadata()?;
    validate_identity_descriptor_metadata(&metadata, euid, false)?;

    // Tighten permissions and truncate on the validated descriptor before writing any secret material.
    file.set_permissions(fs::Permissions::from_mode(CREATED_IDENTITY_FILE_MODE))?;
    file.set_len(0)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(target_family = "windows")]
fn write_identity_file<P: AsRef<Path>>(path: P, contents: &[u8]) -> Result<(), IdentityError> {
    warn_windows_permissions_unforced_once();
    let path = path.as_ref();
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(IdentityError::Symlink);
            }
            if !metadata.is_file() {
                return Err(IdentityError::NotFile);
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => {},
        Err(err) => return Err(IdentityError::Io(err)),
    }
    fs::write(path, contents)?;
    Ok(())
}

/// Check that the given path exists, is a regular file (not a symlink or hard link), is owned by the current effective
/// user, and has no group or other permissions (`mode & 0o077 == 0` on Unix).
#[cfg(any(test, target_family = "windows"))]
fn check_identity_file<P: AsRef<Path>>(path: P) -> Result<(), IdentityError> {
    #[cfg(target_family = "unix")]
    {
        let _file = open_and_validate_identity_file(path.as_ref())?;
        Ok(())
    }
    #[cfg(target_family = "windows")]
    {
        warn_windows_permissions_unforced_once();
        let metadata = match fs::symlink_metadata(path.as_ref()) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Err(IdentityError::NotFound),
            Err(err) => return Err(IdentityError::Io(err)),
        };
        if metadata.file_type().is_symlink() {
            return Err(IdentityError::Symlink);
        }
        if !metadata.is_file() {
            return Err(IdentityError::NotFile);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("Identity file has invalid permissions")]
    InvalidPermissions,
    #[error("Identity path is a symbolic link")]
    Symlink,
    #[error("Identity file has multiple hard links (nlink = {nlink})")]
    HardLink { nlink: u64 },
    #[error("Identity file is owned by uid {actual_uid}, expected current uid {expected_uid}")]
    InvalidOwner { expected_uid: u32, actual_uid: u32 },
    #[error("Identity directory '{path}' is unsafe: {reason}")]
    UnsafeDirectory { path: PathBuf, reason: String },
    #[error("Identity file was not found")]
    NotFound,
    #[error("Path is not a file")]
    NotFile,
    #[error("Malformed identity file at line {line}, column {column}")]
    Malformed { line: usize, column: usize },
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<json5::Error> for IdentityError {
    fn from(err: json5::Error) -> Self {
        match err {
            json5::Error::Message {
                location: Some(loc), ..
            } => IdentityError::Malformed {
                line: loc.line,
                column: loc.column,
            },
            json5::Error::Message { location: None, .. } => IdentityError::Malformed { line: 0, column: 0 },
        }
    }
}

#[cfg(all(test, target_family = "unix"))]
mod test {
    use std::{
        ffi::CString,
        fs::DirBuilder,
        os::unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink},
        },
        path::PathBuf,
    };

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("identity_management_test_{}_{name}", std::process::id()));
        let _unused = fs::remove_dir_all(&dir);
        DirBuilder::new().recursive(true).mode(0o700).create(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn temp_path(name: &str) -> PathBuf {
        temp_dir(name).join("identity.json")
    }

    #[test]
    fn it_creates_the_identity_file_with_the_required_permissions() {
        let path = temp_path("create");
        let _unused = fs::remove_file(&path);

        save_as_json(&path, &"identity").unwrap();

        let mode = fs::symlink_metadata(&path).unwrap().mode() & 0o7777;
        assert_eq!(mode, 0o600);
        check_identity_file(&path).unwrap();
    }

    #[test]
    fn it_tightens_the_permissions_of_an_existing_file_before_writing() {
        let path = temp_path("existing");
        fs::write(&path, b"old contents").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        save_as_json(&path, &"identity").unwrap();

        let mode = fs::symlink_metadata(&path).unwrap().mode() & 0o7777;
        assert_eq!(mode, 0o600);
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.ends_with("\"identity\""), "unexpected contents: {contents}");
    }

    #[test]
    fn malformed_identity_does_not_leak_secret_key() {
        let path = temp_path("malformed_leak");
        let secret_key_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let malformed_content = format!("// Comment line 1\n{{\"secret_key\":\"{secret_key_hex}\",\"broken_json\":");
        fs::write(&path, malformed_content.as_bytes()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let err = load_node_identity(&path, TransportType::Tcp).unwrap_err();
        let err_display = err.to_string();
        assert!(
            !err_display.contains(secret_key_hex),
            "error display must not leak secret key: {err_display}"
        );
        assert!(
            err_display.contains("Malformed identity file at line"),
            "expected line/column in error, got: {err_display}"
        );

        let tor_err = load_from_json::<_, TorIdentity>(&path).unwrap_err();
        let tor_err_display = tor_err.to_string();
        assert!(
            !tor_err_display.contains(secret_key_hex),
            "Tor error display must not leak secret key: {tor_err_display}"
        );
        assert!(
            tor_err_display.contains("Malformed identity file at line"),
            "expected line/column in Tor error, got: {tor_err_display}"
        );
    }

    #[test]
    fn load_from_json_checks_permissions_and_handles_missing_file() {
        let path = temp_path("perm_check");

        // Missing file returns Ok(None)
        let _unused = fs::remove_file(&path);
        assert!(load_from_json::<_, TorIdentity>(&path).unwrap().is_none());
        assert!(load_tor_identity(&path).unwrap().is_none());

        // File with loose permissions (0644) returns Err(IdentityError::InvalidPermissions)
        fs::write(&path, b"{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            load_from_json::<_, TorIdentity>(&path),
            Err(IdentityError::InvalidPermissions)
        ));
        // The Tor loader reports the same actionable error as the node identity
        let err = load_tor_identity(&path).unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        let details = err.details.unwrap();
        assert!(
            details.contains("must be readable only by its owner; run `chmod 600` or `chmod 400`"),
            "expected owner-only chmod 600/400 hint, got: {details}"
        );
        assert!(
            details.contains(&format!("chmod 600 {}", path.to_string_lossy())),
            "expected path and chmod hint, got: {details}"
        );

        // File with strict permissions (0600) succeeds to pass permission gate
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        // `{}` is not a valid TorIdentity, so loading must reach JSON parsing.
        assert!(matches!(
            load_from_json::<_, TorIdentity>(&path),
            Err(IdentityError::Malformed { .. })
        ));
    }

    #[test]
    fn permissions_0400_and_0600_pass_while_0200_0640_and_0644_fail() {
        let path = temp_path("modes_0400_0600_0640_0644");
        let created = create_new_node_identity(&path, vec![], PeerFeatures::COMMUNICATION_NODE).unwrap();
        let original_contents = fs::read_to_string(&path).unwrap();

        // 0600 passes
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        check_identity_file(&path).unwrap();
        let loaded = load_node_identity(&path, TransportType::Tcp).unwrap();
        assert_eq!(loaded.public_key(), created.public_key());

        // 0400 read-only passes end-to-end when no address/signature update is needed
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        check_identity_file(&path).unwrap();
        let loaded_ro = load_node_identity(&path, TransportType::Tcp).unwrap();
        assert_eq!(loaded_ro.public_key(), created.public_key());
        let setup_ro = setup_node_identity(
            &path,
            vec![],
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();
        assert_eq!(setup_ro.public_key(), created.public_key());

        // 0200 write-only is rejected as InvalidPermissions and never overwritten by setup_node_identity
        fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();
        assert!(matches!(
            check_identity_file(&path),
            Err(IdentityError::InvalidPermissions)
        ));
        assert!(matches!(
            load_from_json::<_, NodeIdentity>(&path),
            Err(IdentityError::InvalidPermissions)
        ));
        let err = setup_node_identity(
            &path,
            vec![],
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap_err();
        assert_eq!(err.exit_code, ExitCode::ConfigError);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original_contents);

        // 0640 and 0644 are rejected
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            check_identity_file(&path),
            Err(IdentityError::InvalidPermissions)
        ));
        assert!(matches!(
            load_from_json::<_, NodeIdentity>(&path),
            Err(IdentityError::InvalidPermissions)
        ));

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            check_identity_file(&path),
            Err(IdentityError::InvalidPermissions)
        ));
        assert!(matches!(
            load_from_json::<_, NodeIdentity>(&path),
            Err(IdentityError::InvalidPermissions)
        ));
    }

    #[test]
    fn fifo_at_identity_path_is_rejected_without_blocking() {
        let dir = temp_dir("fifo");
        let fifo_path = dir.join("identity.fifo");
        let c_path = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `c_path` is a valid NUL-terminated path string.
        let rc = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(rc, 0, "mkfifo failed");

        assert!(matches!(check_identity_file(&fifo_path), Err(IdentityError::NotFile)));
        assert!(matches!(
            load_from_json::<_, String>(&fifo_path),
            Err(IdentityError::NotFile)
        ));
        assert!(matches!(
            load_node_identity(&fifo_path, TransportType::Tcp),
            Err(IdentityError::NotFile)
        ));
        assert!(matches!(
            save_as_json(&fifo_path, &"identity"),
            Err(IdentityError::NotFile)
        ));
    }

    #[test]
    fn symlink_at_identity_path_is_rejected_on_load_and_save() {
        let dir = temp_dir("symlink");
        let target = dir.join("target.json");
        save_as_json(&target, &"original").unwrap();
        let original_contents = fs::read_to_string(&target).unwrap();

        let link_path = dir.join("identity_link.json");
        symlink(&target, &link_path).unwrap();

        assert!(matches!(check_identity_file(&link_path), Err(IdentityError::Symlink)));
        assert!(matches!(
            load_from_json::<_, String>(&link_path),
            Err(IdentityError::Symlink)
        ));
        assert!(matches!(
            load_node_identity(&link_path, TransportType::Tcp),
            Err(IdentityError::Symlink)
        ));
        assert!(matches!(
            save_as_json(&link_path, &"overwritten"),
            Err(IdentityError::Symlink)
        ));
        assert_eq!(fs::read_to_string(&target).unwrap(), original_contents);

        // Dangling symlink is also rejected on both load and save without creating the target
        let dangling_target = dir.join("nonexistent_target.json");
        let dangling_link = dir.join("dangling_link.json");
        symlink(&dangling_target, &dangling_link).unwrap();

        assert!(matches!(
            load_from_json::<_, String>(&dangling_link),
            Err(IdentityError::Symlink)
        ));
        assert!(matches!(
            save_as_json(&dangling_link, &"planted"),
            Err(IdentityError::Symlink)
        ));
        assert!(!dangling_target.exists());
    }

    #[test]
    fn hard_link_to_identity_file_is_rejected() {
        let dir = temp_dir("hard_link");
        let path = dir.join("identity.json");
        let hard_link = dir.join("identity_hardlink.json");

        save_as_json(&path, &"identity").unwrap();
        fs::hard_link(&path, &hard_link).unwrap();

        assert!(matches!(
            check_identity_file(&path),
            Err(IdentityError::HardLink { nlink: 2 })
        ));
        assert!(matches!(
            load_from_json::<_, String>(&path),
            Err(IdentityError::HardLink { nlink: 2 })
        ));
        assert!(matches!(
            save_as_json(&path, &"updated"),
            Err(IdentityError::HardLink { nlink: 2 })
        ));
    }

    #[test]
    fn file_owned_by_different_uid_is_rejected() {
        let path = temp_path("different_uid");
        save_as_json(&path, &"identity").unwrap();

        // Direct unit check on the descriptor metadata validator for uid mismatch
        let meta = fs::symlink_metadata(&path).unwrap();
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let euid = unsafe { libc::geteuid() };
        let other_uid = if euid == 0 { 65534 } else { euid.wrapping_add(1) };
        assert!(matches!(
            validate_identity_descriptor_metadata(&meta, other_uid, true),
            Err(IdentityError::InvalidOwner { .. })
        ));

        // End-to-end filesystem check: skipped unless running with the ability to chown
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `c_path` is a valid NUL-terminated path string; `-1` (`gid_t::MAX`) leaves gid unchanged.
        let rc = unsafe { libc::chown(c_path.as_ptr(), other_uid, libc::gid_t::MAX) };
        if rc != 0 {
            return;
        }

        assert!(matches!(
            check_identity_file(&path),
            Err(IdentityError::InvalidOwner {
                expected_uid,
                actual_uid,
            }) if expected_uid == euid && actual_uid == other_uid
        ));
        assert!(matches!(
            load_from_json::<_, String>(&path),
            Err(IdentityError::InvalidOwner { .. })
        ));
        assert!(matches!(
            save_as_json(&path, &"updated"),
            Err(IdentityError::InvalidOwner { .. })
        ));
    }

    #[test]
    fn fresh_identity_directory_is_created_0700() {
        let base = temp_dir("fresh_dir_base");
        let fresh_dir = base.join("nested").join("config");
        let path = fresh_dir.join("identity.json");

        save_as_json(&path, &"identity").unwrap();

        let dir_mode = fs::symlink_metadata(&fresh_dir).unwrap().mode() & 0o7777;
        assert_eq!(dir_mode, 0o700);
    }

    #[test]
    fn existing_0755_identity_directory_fails_with_message_naming_directory() {
        let base = temp_dir("existing_0755_base");
        let loose_dir = base.join("config_0755");
        fs::create_dir_all(&loose_dir).unwrap();
        fs::set_permissions(&loose_dir, fs::Permissions::from_mode(0o755)).unwrap();

        let path = loose_dir.join("identity.json");
        let err = save_as_json(&path, &"identity").unwrap_err();
        assert!(
            matches!(err, IdentityError::UnsafeDirectory { .. }),
            "expected UnsafeDirectory, got: {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains(&loose_dir.to_string_lossy().to_string()),
            "expected error message to name directory '{}', got: {msg}",
            loose_dir.display()
        );
    }
}

#[cfg(all(test, target_family = "windows"))]
mod windows_test {
    use std::{
        path::PathBuf,
        sync::{Mutex, Once},
    };

    use super::*;

    struct RecordingLogger {
        warnings: Mutex<Vec<String>>,
    }

    impl log::Log for RecordingLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Warn
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) && record.target() == LOG_TARGET && record.level() == log::Level::Warn {
                self.warnings.lock().unwrap().push(record.args().to_string());
            }
        }

        fn flush(&self) {}
    }

    static TEST_LOGGER: RecordingLogger = RecordingLogger {
        warnings: Mutex::new(Vec::new()),
    };
    static INIT_LOGGER: Once = Once::new();

    fn init_logger() {
        INIT_LOGGER.call_once(|| {
            let _unused = log::set_logger(&TEST_LOGGER);
            log::set_max_level(log::LevelFilter::Warn);
        });
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("identity_management_win_test_{}_{name}", std::process::id()));
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("identity.json")
    }

    #[test]
    fn windows_logs_permission_warning_exactly_once() {
        init_logger();
        let path = temp_path("warn_once");

        save_as_json(&path, &"identity").unwrap();
        save_as_json(&path, &"identity_2").unwrap();
        check_identity_file(&path).unwrap();
        let loaded: Option<String> = load_from_json(&path).unwrap();
        assert_eq!(loaded.as_deref(), Some("identity_2"));
        let loaded_again: Option<String> = load_from_json(&path).unwrap();
        assert_eq!(loaded_again.as_deref(), Some("identity_2"));

        let warnings = TEST_LOGGER.warnings.lock().unwrap();
        let matching: Vec<&String> = warnings
            .iter()
            .filter(|w| w.contains(WINDOWS_PERMISSION_WARNING))
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "expected Windows permission warning to be logged exactly once, got: {warnings:?}"
        );
    }
}
