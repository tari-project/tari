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
    time::{SystemTime, UNIX_EPOCH},
};

use log::*;
use serde::{Serialize, de::DeserializeOwned};
use tari_common::{
    configuration::bootstrap::{prompt, prompt_default_yes_on_eof},
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

#[cfg(test)]
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
    public_addresses: Vec<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
    transport_type: TransportType,
) -> Result<Arc<NodeIdentity>, ExitError> {
    setup_node_identity_with_prompts(
        identity_file,
        public_addresses,
        create_id,
        peer_features,
        transport_type,
        prompt_default_yes_on_eof,
        prompt,
    )
}

/// Loads the node identity in read-only mode without persisting changes or creating new files on disk.
/// Intended for companion tools such as `minotari_peer_sync` to avoid racing or overwriting the base node's identity.
pub fn setup_node_identity_readonly<P: AsRef<Path>>(
    identity_file: P,
    public_addresses: Vec<Multiaddr>,
    peer_features: PeerFeatures,
    transport_type: TransportType,
) -> Result<Arc<NodeIdentity>, ExitError> {
    match load_node_identity(&identity_file, transport_type, false) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            id.set_public_addresses(filtered_addresses);
            Ok(Arc::new(id))
        },
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            identity_file.as_ref(),
            "a new one will be created on next start",
        )),
        Err(IdentityError::NotFile) => {
            let msg = format!(
                "Node identity path '{}' is not a regular file.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::SymlinkDisallowed) => {
            let msg = format!(
                "Node identity path '{}' is a symlink, which is disallowed for security.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::HardLinkDisallowed) => {
            let msg = format!(
                "Node identity file '{}' has multiple hard links, which is disallowed for security.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::InvalidOwner { expected, actual }) => {
            let msg = format!(
                "Node identity file '{}' is owned by uid {}, but current process is euid {}.",
                identity_file.as_ref().display(),
                actual,
                expected
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(e) => {
            let msg = format!(
                "Failed to load node identity from '{}': {e}",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
    }
}

fn setup_node_identity_with_prompts<P, FMissing, FCorrupt>(
    identity_file: P,
    public_addresses: Vec<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
    transport_type: TransportType,
    prompt_missing: FMissing,
    prompt_corrupt: FCorrupt,
) -> Result<Arc<NodeIdentity>, ExitError>
where
    P: AsRef<Path>,
    FMissing: FnOnce(&str) -> bool,
    FCorrupt: FnOnce(&str) -> bool,
{
    match load_node_identity(&identity_file, transport_type, true) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            // Configured address list is authoritative and replaces the persisted set on load (#8035)
            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            id.set_public_addresses(filtered_addresses);
            Ok(Arc::new(id))
        },
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            identity_file.as_ref(),
            "a new one will be created on next start",
        )),
        Err(IdentityError::NotFile) => {
            let msg = format!(
                "Node identity path '{}' is not a regular file.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::SymlinkDisallowed) => {
            let msg = format!(
                "Node identity path '{}' is a symlink, which is disallowed for security.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::HardLinkDisallowed) => {
            let msg = format!(
                "Node identity file '{}' has multiple hard links, which is disallowed for security.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::InvalidOwner { expected, actual }) => {
            let msg = format!(
                "Node identity file '{}' is owned by uid {}, but current process is euid {}.",
                identity_file.as_ref().display(),
                actual,
                expected
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::UnsafeDirectoryPermissions { path, mode }) => {
            let msg = format!(
                "Identity directory '{}' has unsafe permissions ({mode:o}); must be owned by process euid and not \
                 group- or other-writable.",
                path.display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(IdentityError::ReadOnly { path }) => {
            let msg = format!(
                "Identity file '{}' is read-only; signature or address changes could not be persisted.",
                path.display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(e @ (IdentityError::NotFound | IdentityError::Malformed { .. } | IdentityError::Io(_))) => {
            recover_or_create_node_identity(
                identity_file.as_ref(),
                public_addresses,
                create_id,
                peer_features,
                transport_type,
                &e,
                prompt_missing,
                prompt_corrupt,
            )
        },
    }
}

fn recover_or_create_node_identity<FMissing, FCorrupt>(
    identity_file: &Path,
    public_addresses: Vec<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
    transport_type: TransportType,
    load_err: &IdentityError,
    prompt_missing: FMissing,
    prompt_corrupt: FCorrupt,
) -> Result<Arc<NodeIdentity>, ExitError>
where
    FMissing: FnOnce(&str) -> bool,
    FCorrupt: FnOnce(&str) -> bool,
{
    let is_missing = matches!(load_err, IdentityError::NotFound);
    if !create_id {
        let response = if is_missing {
            prompt_missing("Node identity does not exist.\nWould you like to create one (Y/n)?")
        } else {
            prompt_corrupt(
                "Node identity could not be loaded.\nWould you like to back it up and create a new one (y/N)?",
            )
        };
        if !response {
            error!(
                target: LOG_TARGET,
                "Node identity not found. {load_err}. You can update the configuration file to point to a valid node \
                 identity file, or re-run the node and create a new one."
            );
            return Err(ExitError::new(
                ExitCode::ConfigError,
                format!(
                    "Node identity information not found. {load_err}. You can update the configuration file to point \
                     to a valid node identity file, or re-run the node to create a new one"
                ),
            ));
        };
    }

    let backup_path = if is_missing {
        None
    } else {
        let backup = backup_identity_file(identity_file).map_err(|(backup, rename_err)| {
            error!(
                target: LOG_TARGET,
                "Could not back up existing node identity file '{}' to '{}' (note: backing up an existing \
                 identity requires a filesystem that supports hard links): {rename_err}",
                identity_file.display(),
                backup.display(),
            );
            ExitError::new(
                ExitCode::ConfigError,
                format!(
                    "Could not back up existing node identity file '{}' to '{}' (note: backing up an existing \
                     identity requires a filesystem that supports hard links): {rename_err}",
                    identity_file.display(),
                    backup.display(),
                ),
            )
        })?;
        Some(backup)
    };

    let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
    match create_new_node_identity(identity_file, filtered_addresses, peer_features) {
        Ok(id) => {
            if let Some(backup) = backup_path {
                error!(
                    target: LOG_TARGET,
                    "Failed to load node identity ({load_err}); backed up existing file to {} and created new node \
                     identity [{}] with public key {} at {}.",
                    backup.display(),
                    id.node_id(),
                    id.public_key(),
                    identity_file.display(),
                );
            } else {
                debug!(target: LOG_TARGET, "Existing node id not found. {load_err}. Creating new ID");
                info!(
                    target: LOG_TARGET,
                    "New node identity [{}] with public key {} has been created at {}.",
                    id.node_id(),
                    id.public_key(),
                    identity_file.to_str().unwrap_or("?"),
                );
            }
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
}

/// Tries to construct a node identity by loading the secret key and other metadata from disk and calculating the
/// missing fields from that information.
///
/// ## Parameters
/// `path` - Reference to a path
///
/// ## Returns
/// Result containing a NodeIdentity on success, string indicates the reason on failure
fn load_node_identity<P: AsRef<Path>>(
    path: P,
    transport_type: TransportType,
    allow_persist: bool,
) -> Result<NodeIdentity, IdentityError> {
    let id_str = read_identity_file(path.as_ref())?;
    let id = json5::from_str::<NodeIdentity>(&id_str)?;

    let mut changed = false;

    // Check whether the previous version has a signature and sign if necessary
    if !id.is_signed() {
        id.sign();
        changed = true;
    }
    debug!(
        target: LOG_TARGET,
        "Node ID loaded with public key {} and Node id {}",
        id.public_key().to_hex(),
        id.node_id().to_hex()
    );
    if changed {
        if allow_persist {
            save_as_json(&path, &id)?;
        } else {
            return Err(IdentityError::ReadOnly {
                path: path.as_ref().to_path_buf(),
            });
        }
    }

    // In-memory TCP filter only; never persists the stripped list to disk (#8035)
    if transport_type == TransportType::Tcp {
        let current_addresses = id.public_addresses();
        let filtered_addresses = filter_addresses_for_transport(current_addresses, transport_type);
        id.set_public_addresses(filtered_addresses);
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
    if !path.as_ref().exists() {
        return Ok(None);
    }

    check_identity_file(&path)?;

    let contents = fs::read_to_string(path)?;
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

/// Builds a candidate backup path `<name>.<unix-timestamp>.bak` (or `<name>.<unix-timestamp>.<attempt>.bak` when an
/// earlier backup from the same second already exists) in the same directory as `path`.
fn backup_identity_path_candidate(path: &Path, timestamp: u64, attempt: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "identity.json".into());
    if attempt == 0 {
        path.with_file_name(format!("{file_name}.{timestamp}.bak"))
    } else {
        path.with_file_name(format!("{file_name}.{timestamp}.{attempt}.bak"))
    }
}

/// Moves `path` to a new `<name>.<unix-timestamp>[.<attempt>].bak` file in the same directory without overwriting any
/// existing backup file.
fn backup_identity_file(path: &Path) -> Result<PathBuf, (PathBuf, io::Error)> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for attempt in 0..=u32::MAX {
        let candidate = backup_identity_path_candidate(path, timestamp, attempt);
        match move_file_no_overwrite(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err((candidate, err)),
        }
    }
    let fallback = backup_identity_path_candidate(path, timestamp, u32::MAX);
    Err((
        fallback,
        io::Error::new(io::ErrorKind::AlreadyExists, "exhausted backup filename attempts"),
    ))
}

#[cfg(target_family = "unix")]
fn move_file_no_overwrite(src: &Path, dst: &Path) -> io::Result<()> {
    // `hard_link` atomically fails with `AlreadyExists` if `dst` already exists, never overwriting an earlier backup.
    fs::hard_link(src, dst)?;
    if let Err(err) = fs::remove_file(src) {
        let _unused = fs::remove_file(dst);
        return Err(err);
    }
    Ok(())
}

#[cfg(target_family = "windows")]
fn move_file_no_overwrite(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "backup file already exists",
        ));
    }
    fs::rename(src, dst)
}

/// The error returned when an identity file has permissions other than 0600 or 0400, telling the user how to fix it.
/// `recreated` describes what happens if the user moves the file aside instead.
fn invalid_permissions_error(path: &Path, recreated: &str) -> ExitError {
    ExitError::new(
        ExitCode::ConfigError,
        format!(
            "{path} has incorrect permissions: it must be readable only by its owner; run 'chmod 600 {path}' or \
             'chmod 400 {path}', or move the identity file aside and {recreated}",
            path = path.to_string_lossy()
        ),
    )
}

#[cfg(target_family = "unix")]
fn ensure_secure_directory(dir: &Path) -> Result<(), IdentityError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    if !dir.exists() {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }

    let meta = fs::metadata(dir)?;
    let current_euid = unsafe { libc::geteuid() };
    if meta.uid() != current_euid {
        return Err(IdentityError::InvalidOwner {
            expected: current_euid,
            actual: meta.uid(),
        });
    }

    let mode = meta.mode() & 0o777;
    if (mode & 0o022) != 0 {
        return Err(IdentityError::UnsafeDirectoryPermissions {
            path: dir.to_path_buf(),
            mode,
        });
    }

    Ok(())
}

#[cfg(target_family = "windows")]
fn ensure_secure_directory(dir: &Path) -> Result<(), IdentityError> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
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
    let p = path.as_ref();
    let parent = p.parent().unwrap_or_else(|| Path::new("."));
    ensure_secure_directory(parent)?;

    let json = json5::to_string(object)?;
    let json_with_comment =
        format!("// This file is generated by the Minotari base node. Any changes will be overwritten.\n{json}");
    write_identity_file(p, json_with_comment.as_bytes())?;
    Ok(())
}

#[cfg(target_family = "unix")]
fn write_identity_file(path: &Path, contents: &[u8]) -> Result<(), IdentityError> {
    use std::{
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "identity.json".into());
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = parent.join(format!(".{file_name}.{pid}.{ts}.tmp"));

    let write_res = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp_path)?;

        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        fs::rename(&tmp_path, path)?;

        if let Ok(dir_file) = fs::File::open(parent) {
            let _ = dir_file.sync_all();
        }
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        if e.kind() == io::ErrorKind::PermissionDenied {
            return Err(IdentityError::ReadOnly {
                path: path.to_path_buf(),
            });
        }
        return Err(IdentityError::Io(e));
    }

    Ok(())
}

#[cfg(target_family = "windows")]
fn write_identity_file(path: &Path, contents: &[u8]) -> Result<(), IdentityError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "identity.json".into());
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = parent.join(format!(".{file_name}.{pid}.{ts}.tmp"));

    let write_res = (|| -> io::Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        if e.kind() == io::ErrorKind::PermissionDenied {
            return Err(IdentityError::ReadOnly {
                path: path.to_path_buf(),
            });
        }
        return Err(IdentityError::Io(e));
    }
    Ok(())
}

#[cfg(target_family = "unix")]
fn open_and_validate_identity_file(path: &Path) -> Result<fs::File, IdentityError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Err(IdentityError::SymlinkDisallowed),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(IdentityError::NotFound),
        Err(e) => return Err(IdentityError::Io(e)),
    };

    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(IdentityError::NotFile);
    }

    let current_euid = unsafe { libc::geteuid() };
    if meta.uid() != current_euid {
        return Err(IdentityError::InvalidOwner {
            expected: current_euid,
            actual: meta.uid(),
        });
    }

    if meta.nlink() > 1 {
        return Err(IdentityError::HardLinkDisallowed);
    }

    let mode = meta.mode() & 0o777;
    if (mode & 0o077) != 0 || (mode & 0o400) == 0 {
        return Err(IdentityError::InvalidPermissions);
    }

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

#[cfg(target_family = "unix")]
fn check_identity_file<P: AsRef<Path>>(path: P) -> Result<(), IdentityError> {
    open_and_validate_identity_file(path.as_ref()).map(|_| ())
}

#[cfg(target_family = "windows")]
fn check_identity_file<P: AsRef<Path>>(path: P) -> Result<(), IdentityError> {
    let p = path.as_ref();
    if !p.exists() {
        return Err(IdentityError::NotFound);
    }
    if !p.is_file() {
        return Err(IdentityError::NotFile);
    }
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        warn!(target: LOG_TARGET, "Identity file permissions cannot be strictly checked on Windows: {}", p.display());
    }
    Ok(())
}

#[cfg(target_family = "windows")]
fn read_identity_file(path: &Path) -> Result<String, IdentityError> {
    check_identity_file(path)?;
    let contents = fs::read_to_string(path)?;
    Ok(contents)
}

#[cfg(all(test, target_family = "unix"))]
fn has_permissions<P: AsRef<Path>>(path: P, perms: u32) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    Ok(metadata.permissions().mode() == perms)
}

#[cfg(all(test, target_family = "windows"))]
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
    #[error("Identity path is a symlink, which is disallowed for security")]
    SymlinkDisallowed,
    #[error("Identity file has multiple hard links, which is disallowed for security")]
    HardLinkDisallowed,
    #[error("Identity file is owned by uid {actual}, expected {expected}")]
    InvalidOwner { expected: u32, actual: u32 },
    #[error("Identity directory '{path}' has unsafe permissions ({mode:o})")]
    UnsafeDirectoryPermissions { path: PathBuf, mode: u32 },
    #[error("Identity file '{path}' is read-only and cannot be updated")]
    ReadOnly { path: PathBuf },
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
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        test_dir(name).join("identity.json")
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("identity_management_test_{name}"));
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn it_creates_the_identity_file_with_the_required_permissions() {
        let path = temp_path("create");
        let _unused = fs::remove_file(&path);

        save_as_json(&path, &"identity").unwrap();

        assert!(has_permissions(&path, REQUIRED_IDENTITY_PERMS).unwrap());
        check_identity_file(&path).unwrap();
    }

    #[test]
    fn it_tightens_the_permissions_of_an_existing_file_before_writing() {
        let path = temp_path("existing");
        fs::write(&path, b"old contents").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        save_as_json(&path, &"identity").unwrap();

        assert!(has_permissions(&path, REQUIRED_IDENTITY_PERMS).unwrap());
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

        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();
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
            details.contains(&format!("chmod 600 {}", path.to_string_lossy())),
            "expected path and chmod hint, got: {details}"
        );

        assert!(
            !details.to_lowercase().contains("delete"),
            "permission error message must not advise deleting the identity file: {details}"
        );
        assert!(
            details.contains("or move the identity file aside"),
            "permission error message must advise moving the identity file aside: {details}"
        );

        // File with strict permissions (0600) succeeds to pass permission gate
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        // `{}` is not a valid TorIdentity, so loading must reach JSON parsing.
        assert!(matches!(
            load_from_json::<_, TorIdentity>(&path),
            Err(IdentityError::Malformed { .. })
        ));
    }

    struct ThreadLocalLogger;

    thread_local! {
        static CAPTURED_LOGS: std::cell::RefCell<Vec<(Level, String)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    impl Log for ThreadLocalLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            if record.target() == LOG_TARGET {
                CAPTURED_LOGS.with(|logs| {
                    logs.borrow_mut().push((record.level(), record.args().to_string()));
                });
            }
        }

        fn flush(&self) {}
    }

    static LOGGER_INIT: std::sync::Once = std::sync::Once::new();
    static LOGGER: ThreadLocalLogger = ThreadLocalLogger;

    fn with_captured_logs<F, R>(f: F) -> (R, Vec<(Level, String)>)
    where F: FnOnce() -> R {
        LOGGER_INIT.call_once(|| {
            let _unused = log::set_logger(&LOGGER);
            log::set_max_level(LevelFilter::Debug);
        });
        CAPTURED_LOGS.with(|logs| logs.borrow_mut().clear());
        let result = f();
        let captured = CAPTURED_LOGS.with(|logs| logs.borrow().clone());
        (result, captured)
    }

    #[test]
    fn truncated_identity_file_is_backed_up_and_regenerated_with_error_log() {
        let dir = test_dir("truncated_backup");
        let path = dir.join("identity.json");

        let original_bytes = b"// truncated identity\n{\"node_id\":\"deadbeef\"";
        fs::write(&path, original_bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let (res, logs) = with_captured_logs(|| {
            setup_node_identity(
                &path,
                Vec::new(),
                true,
                PeerFeatures::COMMUNICATION_NODE,
                TransportType::Tcp,
            )
        });
        let new_id = res.expect("setup_node_identity should back up and create a new identity");

        // Find the .bak file in `dir`
        let bak_files: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bak"))
            .collect();
        assert_eq!(
            bak_files.len(),
            1,
            "expected exactly one .bak file, found: {bak_files:?}"
        );
        let bak_path = bak_files.first().unwrap();
        assert_eq!(fs::read(bak_path).unwrap(), original_bytes);

        // New identity file exists and loads cleanly into the newly generated identity
        let reloaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(reloaded.node_id(), new_id.node_id());
        assert_eq!(reloaded.public_key(), new_id.public_key());

        // Exactly one error log line naming the backup path, new node_id, and new public_key, and no warn log line
        let error_logs: Vec<&String> = logs
            .iter()
            .filter_map(|(lvl, msg)| (*lvl == Level::Error).then_some(msg))
            .collect();
        let warn_logs: Vec<&String> = logs
            .iter()
            .filter_map(|(lvl, msg)| (*lvl == Level::Warn).then_some(msg))
            .collect();
        assert!(warn_logs.is_empty(), "expected no warn logs, got: {warn_logs:?}");
        assert_eq!(error_logs.len(), 1, "expected one error log line, got: {error_logs:?}");
        let err_log = error_logs.first().unwrap();
        assert!(
            err_log.contains(&bak_path.display().to_string()),
            "error log must name backup path {}: {}",
            bak_path.display(),
            err_log
        );
        assert!(
            err_log.contains(&new_id.node_id().to_string()),
            "error log must name new node id: {err_log}"
        );
        assert!(
            err_log.contains(&new_id.public_key().to_string()),
            "error log must name new public key: {err_log}"
        );
    }

    #[test]
    fn directory_at_identity_path_returns_exit_error_and_creates_nothing() {
        let dir = test_dir("dir_at_path");
        let identity_dir = dir.join("identity.json");
        fs::create_dir_all(&identity_dir).unwrap();

        let err = setup_node_identity(
            &identity_dir,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert!(
            identity_dir.is_dir(),
            "directory at identity path must not be moved or replaced"
        );

        let entries: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        assert_eq!(entries, vec![identity_dir]);
    }

    #[test]
    fn missing_identity_file_creates_new_without_backup() {
        let dir = test_dir("missing_creates_new");
        let path = dir.join("identity.json");

        let id = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .expect("missing identity file should create a new one");
        assert!(path.is_file());
        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded.node_id(), id.node_id());

        let bak_files: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bak"))
            .collect();
        assert!(
            bak_files.is_empty(),
            "no backup file should be created when original was missing"
        );
    }

    #[test]
    fn failed_backup_rename_in_unwritable_parent_dir_aborts_without_overwriting() {
        let dir = test_dir("unwritable_parent");
        let path = dir.join("identity.json");

        let original_bytes = b"// corrupt identity\n{broken";
        fs::write(&path, original_bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        if unsafe { geteuid() } == 0 {
            return;
        }

        // Make parent directory read+search only (0500) so renaming to .bak fails.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();

        let res = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        );

        // Restore parent directory permissions before asserting so cleanup always succeeds.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();

        let err = res.expect_err("must fail when renaming to .bak fails");
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert_eq!(
            fs::read(&path).unwrap(),
            original_bytes,
            "original corrupt file must remain untouched when backup rename fails"
        );
    }

    #[test]
    fn multiple_backups_in_same_second_do_not_overwrite_each_other() {
        let dir = test_dir("same_second_backups");
        let path = dir.join("identity.json");

        let first_corrupt = b"// first corrupt identity\n{first";
        fs::write(&path, first_corrupt).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .expect("first backup and recreate should succeed");

        let second_corrupt = b"// second corrupt identity\n{second";
        fs::write(&path, second_corrupt).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .expect("second backup and recreate should succeed");

        let mut bak_contents: Vec<Vec<u8>> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bak"))
            .map(|p| fs::read(p).unwrap())
            .collect();
        bak_contents.sort();

        let mut expected = vec![first_corrupt.to_vec(), second_corrupt.to_vec()];
        expected.sort();
        assert_eq!(
            bak_contents, expected,
            "both corrupt identity files must be preserved in distinct .bak files"
        );
    }

    #[test]
    fn missing_identity_with_eof_on_stdin_creates_identity_while_corrupt_aborts() {
        let dir = test_dir("eof_stdin_split");
        let path = dir.join("identity.json");

        // Missing file + create_id == false + EOF on stdin (simulated via prompt_default_yes_on_eof -> true)
        // creates the identity on first start without a terminal.
        let created = setup_node_identity_with_prompts(
            &path,
            Vec::new(),
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            |_| true,
            |_| false,
        )
        .expect("missing identity with EOF-means-yes must create identity on first start");
        assert!(path.is_file());
        assert_eq!(
            load_node_identity(&path, TransportType::Tcp, true).unwrap().node_id(),
            created.node_id()
        );

        // Corrupt file + create_id == false + EOF on stdin (simulated via prompt -> false)
        // refuses to replace the corrupt identity.
        let corrupt_bytes = b"// corrupt identity\n{broken";
        fs::write(&path, corrupt_bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let err = setup_node_identity_with_prompts(
            &path,
            Vec::new(),
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            |_| true,
            |_| false,
        )
        .expect_err("corrupt identity with EOF-means-no must abort");
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert_eq!(fs::read(&path).unwrap(), corrupt_bytes);
    }

    #[test]
    fn unchanged_load_performs_no_write() {
        use std::os::unix::fs::MetadataExt;
        let dir = test_dir("unchanged_load_no_write");
        let path = dir.join("identity.json");

        let _id = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();

        let meta_before = fs::metadata(&path).unwrap();
        let ino_before = meta_before.ino();
        let mtime_before = meta_before.modified().unwrap();

        let _reloaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();

        let meta_after = fs::metadata(&path).unwrap();
        assert_eq!(
            meta_after.ino(),
            ino_before,
            "inode must not change on read without modification"
        );
        assert_eq!(
            meta_after.modified().unwrap(),
            mtime_before,
            "mtime must not change on read without modification"
        );
    }

    fn write_unsigned_identity_file(path: &Path) {
        let id =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        let json = json5::to_string(&id).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("identity_signature".to_string(), serde_json::Value::Null);
        }
        let unsigned_json = serde_json::to_string(&v).unwrap();
        write_identity_file(path, unsigned_json.as_bytes()).unwrap();
    }

    #[test]
    fn load_and_sign_writes_via_temp_file_and_rename() {
        use std::os::unix::fs::MetadataExt;
        let dir = test_dir("sign_atomic_rename");
        let path = dir.join("identity.json");

        write_unsigned_identity_file(&path);

        let meta_before = fs::metadata(&path).unwrap();
        let ino_before = meta_before.ino();

        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert!(loaded.is_signed());

        let meta_after = fs::metadata(&path).unwrap();
        assert_ne!(meta_after.ino(), ino_before, "atomic rename should replace the inode");

        let tmp_count = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(tmp_count, 0, "no temporary files must be left behind");
    }

    #[test]
    fn readonly_0400_signed_identity_loads_successfully() {
        let dir = test_dir("readonly_0400_signed");
        let path = dir.join("identity.json");

        let id = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();

        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded.node_id(), id.node_id());
    }

    #[test]
    fn readonly_0400_needing_signature_returns_readonly_error() {
        let dir = test_dir("readonly_0400_needs_sig");
        let path = dir.join("identity.json");

        write_unsigned_identity_file(&path);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let err = load_node_identity(&path, TransportType::Tcp, false).unwrap_err();
        assert!(matches!(err, IdentityError::ReadOnly { .. }));
    }

    #[test]
    fn symlink_identity_is_rejected() {
        let dir = test_dir("symlink_rejected");
        let real_path = dir.join("real_identity.json");
        let sym_path = dir.join("sym_identity.json");

        let _id = setup_node_identity(
            &real_path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();

        std::os::unix::fs::symlink(&real_path, &sym_path).unwrap();

        let err = load_node_identity(&sym_path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::SymlinkDisallowed));
    }

    #[test]
    fn hardlink_identity_is_rejected() {
        let dir = test_dir("hardlink_rejected");
        let real_path = dir.join("identity.json");
        let hard_link_path = dir.join("hard_identity.json");

        let _id = setup_node_identity(
            &real_path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();

        fs::hard_link(&real_path, &hard_link_path).unwrap();

        let err = load_node_identity(&real_path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::HardLinkDisallowed));
    }

    #[test]
    fn secure_0700_directory_creation_and_unsafe_parent_rejected() {
        use std::os::unix::fs::MetadataExt;
        let base_dir = test_dir("dir_perms_test");

        let secure_sub = base_dir.join("sub_0700");
        let secure_file = secure_sub.join("identity.json");

        save_as_json(&secure_file, &"test").unwrap();
        let meta = fs::metadata(&secure_sub).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o700);

        let unsafe_sub = base_dir.join("sub_unsafe");
        fs::create_dir_all(&unsafe_sub).unwrap();
        fs::set_permissions(&unsafe_sub, fs::Permissions::from_mode(0o777)).unwrap();
        let unsafe_file = unsafe_sub.join("identity.json");

        let err = save_as_json(&unsafe_file, &"test").unwrap_err();
        assert!(matches!(err, IdentityError::UnsafeDirectoryPermissions { .. }));
    }

    #[test]
    fn configured_addresses_override_persisted_without_destructive_write() {
        let dir = test_dir("address_authority_test");
        let path = dir.join("identity.json");

        let onion_addr: Multiaddr = "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234"
            .parse()
            .unwrap();
        let tcp_addr: Multiaddr = "/ip4/198.51.100.1/tcp/18141".parse().unwrap();

        let _id = setup_node_identity(
            &path,
            vec![onion_addr.clone()],
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tor,
        )
        .unwrap();

        let raw_disk = fs::read_to_string(&path).unwrap();
        assert!(raw_disk.contains("onion3"));

        let loaded = setup_node_identity(
            &path,
            vec![tcp_addr.clone()],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();

        assert_eq!(loaded.public_addresses(), vec![tcp_addr]);

        let raw_disk_after = fs::read_to_string(&path).unwrap();
        assert!(
            raw_disk_after.contains("onion3"),
            "persisted identity must retain onion addresses"
        );
    }
}
