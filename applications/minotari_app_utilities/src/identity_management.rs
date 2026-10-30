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
    collections::HashSet,
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

const REQUIRED_IDENTITY_PERMS: u32 = 0o100600;
const READONLY_IDENTITY_PERMS: u32 = 0o100400;

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
    match load_node_identity(&identity_file, transport_type) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            // Filter addresses based on transport type
            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            if !filtered_addresses.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "Setting configured public addresses: {:?}",
                    filtered_addresses
                );
                id.set_public_addresses(filtered_addresses);
            } else if !id.public_addresses().is_empty() {
                warn!(
                    target: LOG_TARGET,
                    "No `p2p.public_addresses` configured; keeping existing addresses {:?} from identity file '{}'. \
                     Please move them into `p2p.public_addresses` in config.toml.",
                    id.public_addresses(),
                    identity_file.as_ref().display()
                );
            } else {
                // No configured or persisted public addresses
            }
            Ok(Arc::new(id))
        },
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            identity_file.as_ref(),
            "a new one will be created on next start",
        )),
        Err(e @ IdentityError::ReadOnly { .. }) => {
            error!(target: LOG_TARGET, "{e}");
            Err(ExitError::new(ExitCode::ConfigError, e.to_string()))
        },
        Err(IdentityError::NotFile) => {
            let msg = format!(
                "Node identity path '{}' is not a regular file.",
                identity_file.as_ref().display()
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

/// Loads the node identity without ever writing to or creating the identity file on disk.
///
/// Used by auxiliary tools such as `minotari_peer_sync --use-node-identity` so they cannot race a running base
/// node's identity file or fail when the identity file is mounted read-only (`0400`).
pub fn setup_node_identity_read_only<P: AsRef<Path>>(
    identity_file: P,
    public_addresses: Vec<Multiaddr>,
    peer_features: PeerFeatures,
    transport_type: TransportType,
) -> Result<Arc<NodeIdentity>, ExitError> {
    match load_node_identity_inner(&identity_file, transport_type, false) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            let filtered_addresses = filter_addresses_for_transport(public_addresses, transport_type);
            if !filtered_addresses.is_empty() {
                id.set_public_addresses(filtered_addresses);
            }
            Ok(Arc::new(id))
        },
        Err(IdentityError::InvalidPermissions) => Err(invalid_permissions_error(
            identity_file.as_ref(),
            "a new one will be created on next start",
        )),
        Err(e @ IdentityError::ReadOnly { .. }) => {
            error!(target: LOG_TARGET, "{e}");
            Err(ExitError::new(ExitCode::ConfigError, e.to_string()))
        },
        Err(e) => {
            error!(target: LOG_TARGET, "Failed to load node identity in read-only mode: {e}");
            Err(ExitError::new(
                ExitCode::ConfigError,
                format!("Failed to load node identity: {e}"),
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
fn load_node_identity<P: AsRef<Path>>(path: P, transport_type: TransportType) -> Result<NodeIdentity, IdentityError> {
    load_node_identity_inner(path, transport_type, true)
}

fn load_node_identity_inner<P: AsRef<Path>>(
    path: P,
    transport_type: TransportType,
    persist: bool,
) -> Result<NodeIdentity, IdentityError> {
    check_identity_file(&path)?;

    let id_str = fs::read_to_string(path.as_ref())?;
    let id = json5::from_str::<NodeIdentity>(&id_str)?;

    // Check whether the previous version has a signature and sign if necessary.
    // Save only when a signature was added (and `persist` is enabled), BEFORE applying
    // the in-memory TCP transport filter so onion addresses are never stripped from disk.
    if !id.is_signed() {
        id.sign();
        if persist {
            save_as_json(&path, &id)?;
        }
    }

    if transport_type == TransportType::Tcp {
        let current_addresses = id.public_addresses();
        debug!(
            target: LOG_TARGET,
            "Filtering addresses for TCP transport. Current addresses: {:?}",
            current_addresses
        );
        // For TCP transport remove all onion addresses in memory only
        let filtered_addresses: Vec<Multiaddr> = current_addresses
            .into_iter()
            .filter(|addr| !addr.iter().any(|p| matches!(p, Protocol::Onion3(_))))
            .collect();
        debug!(
            target: LOG_TARGET,
            "After filtering for TCP transport, {} addresses remain: {:?}",
            filtered_addresses.len(),
            filtered_addresses
        );
        id.set_public_addresses(filtered_addresses);
    }

    debug!(
        target: LOG_TARGET,
        "Node ID loaded with public key {} and Node id {}",
        id.public_key().to_hex(),
        id.node_id().to_hex()
    );
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

/// The error returned when an identity file has permissions other than 0600, telling the user how to fix it.
/// `recreated` describes what happens if the user moves the file aside instead.
fn invalid_permissions_error(path: &Path, recreated: &str) -> ExitError {
    ExitError::new(
        ExitCode::ConfigError,
        format!(
            "{path} has incorrect permissions. You can update the identity file with the correct permissions using \
             'chmod 600 {path}', or move the identity file aside and {recreated}",
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
    let json_with_comment =
        format!("// This file is generated by the Minotari base node. Any changes will be overwritten.\n{json}");
    if is_unchanged_identity_file(path.as_ref(), &json, &json_with_comment) {
        return Ok(());
    }
    if let Some(p) = path.as_ref().parent() &&
        !p.as_os_str().is_empty() &&
        !p.exists()
    {
        fs::create_dir_all(p).map_err(|e| map_write_io_error(path.as_ref(), e))?;
    }
    write_identity_file(path.as_ref(), json_with_comment.as_bytes())
        .map_err(|e| map_write_io_error(path.as_ref(), e))?;
    Ok(())
}

/// Returns `true` when `path` already exists with valid identity permissions (`0600` or `0400`) and its contents
/// (or deserialized `NodeIdentity` public key, features, and set of public addresses when signed) already match
/// `json`, allowing no-op re-saves (such as post-comms Tor startup with a read-only `0400` identity file where
/// `add_public_address` re-signs with a fresh timestamp) to succeed without rewriting the file.
fn is_unchanged_identity_file(path: &Path, json: &str, json_with_comment: &str) -> bool {
    if check_identity_file(path).is_err() {
        return false;
    }
    let Ok(existing) = fs::read_to_string(path) else {
        return false;
    };
    if existing == json_with_comment || existing.trim() == json.trim() {
        return true;
    }
    if let (Ok(existing_id), Ok(new_id)) = (
        json5::from_str::<NodeIdentity>(&existing),
        json5::from_str::<NodeIdentity>(json),
    ) {
        let existing_addrs = existing_id.public_addresses();
        let new_addrs = new_id.public_addresses();
        let existing_set: HashSet<&Multiaddr> = existing_addrs.iter().collect();
        let new_set: HashSet<&Multiaddr> = new_addrs.iter().collect();
        return existing_id.is_signed() &&
            new_id.is_signed() &&
            existing_id.public_key() == new_id.public_key() &&
            existing_id.features() == new_id.features() &&
            existing_addrs.len() == new_addrs.len() &&
            existing_set == new_set;
    }
    false
}

fn map_write_io_error(path: &Path, err: io::Error) -> IdentityError {
    if err.kind() == io::ErrorKind::PermissionDenied {
        IdentityError::ReadOnly {
            path: path.to_path_buf(),
        }
    } else {
        IdentityError::Io(err)
    }
}

fn temp_identity_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "identity path has no valid file name"))?;
    Ok(path.with_file_name(format!("{file_name}.tmp")))
}

/// Resolves a symlinked identity file to its target path so atomic writes create the temporary file in the target's
/// parent directory and update the target file without replacing the symlink itself.
fn resolve_identity_write_target(path: &Path) -> PathBuf {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) &&
        let Ok(resolved) = fs::canonicalize(path)
    {
        return resolved;
    }
    path.to_path_buf()
}

#[cfg(target_family = "unix")]
fn open_temp_identity_file(path: &Path, mode: u32) -> io::Result<(PathBuf, fs::File)> {
    use std::os::unix::fs::OpenOptionsExt;

    let tmp_path = temp_identity_path(path)?;
    let open_new = |candidate: &Path| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(candidate)
    };

    match open_new(&tmp_path) {
        Ok(file) => return Ok((tmp_path, file)),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(&tmp_path).is_ok_and(|m| m.file_type().is_file()) {
                let _unused = fs::remove_file(&tmp_path);
                match open_new(&tmp_path) {
                    Ok(file) => return Ok((tmp_path, file)),
                    Err(retry_err) if retry_err.kind() != io::ErrorKind::AlreadyExists => return Err(retry_err),
                    Err(_) => {},
                }
            }
        },
        Err(err) => return Err(err),
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "identity path has no valid file name"))?;
    let pid = std::process::id();
    for attempt in 1..=u32::MAX {
        let candidate = path.with_file_name(format!("{file_name}.tmp.{pid}.{attempt}"));
        match open_new(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "exhausted temporary identity filename attempts",
    ))
}

#[cfg(target_family = "unix")]
fn is_bind_mount_or_cross_device_rename_error(err: &io::Error) -> bool {
    // EBUSY (16 on Linux/macOS) or EXDEV (18 on Linux/macOS) when the identity file is bind-mounted on its own
    // (e.g. `docker run -v ./base_node_id.json:/var/tari/config/base_node_id.json`).
    matches!(err.raw_os_error(), Some(16 | 18)) ||
        err.kind() == io::ErrorKind::ResourceBusy ||
        err.kind() == io::ErrorKind::CrossesDevices
}

#[cfg(target_family = "unix")]
fn write_identity_file_in_place(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    use std::{
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

/// Writes the identity file atomically via `<name>.tmp` in the same directory with `create_new(true)` and
/// `mode(0o600)`, `write_all`, `sync_all`, then `fs::rename` onto the target and fsyncs the directory.
/// Falls back to an in-place write (`O_TRUNC` + write + `fsync`) with a warning if `rename` fails with `EBUSY`
/// or `EXDEV` (such as a single-file Docker bind mount).
#[cfg(target_family = "unix")]
fn write_identity_file<P: AsRef<Path>>(path: P, contents: &[u8]) -> io::Result<()> {
    write_identity_file_with_hooks(path.as_ref(), contents, |_| Ok(()), |src, dst| fs::rename(src, dst))
}

#[cfg(all(test, target_family = "unix"))]
fn write_identity_file_with_hook<F>(path: &Path, contents: &[u8], before_rename: F) -> io::Result<()>
where F: FnOnce(&Path) -> io::Result<()> {
    write_identity_file_with_hooks(path, contents, before_rename, |src, dst| fs::rename(src, dst))
}

#[cfg(target_family = "unix")]
fn write_identity_file_with_hooks<F, R>(path: &Path, contents: &[u8], before_rename: F, rename_fn: R) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
    R: FnOnce(&Path, &Path) -> io::Result<()>,
{
    use std::{io::Write, os::unix::fs::PermissionsExt};

    let target_path = resolve_identity_write_target(path);

    // On Unix, `rename(2)` checks write permission on the parent directory rather than on the target file.
    // Explicitly reject overwriting a read-only (`0400`) target file so callers receive `PermissionDenied`
    // (`IdentityError::ReadOnly`) and the file on disk remains untouched.
    if let Ok(meta) = fs::metadata(&target_path) &&
        (meta.permissions().mode() & 0o200) == 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "identity file is read-only",
        ));
    }

    let mode = REQUIRED_IDENTITY_PERMS & 0o7777;
    let (tmp_path, mut file) = match open_temp_identity_file(&target_path, mode) {
        Ok(opened) => opened,
        Err(err) if is_bind_mount_or_cross_device_rename_error(&err) => {
            warn!(
                target: LOG_TARGET,
                "Could not create temporary identity file alongside '{}' ({err}); falling back to non-atomic \
                 in-place write",
                target_path.display()
            );
            return write_identity_file_in_place(&target_path, contents, mode);
        },
        Err(err) => return Err(err),
    };

    let write_res = (|| -> io::Result<()> {
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    if let Err(e) = write_res {
        let _unused = fs::remove_file(&tmp_path);
        return Err(e);
    }

    before_rename(&tmp_path)?;

    match rename_fn(&tmp_path, &target_path) {
        Ok(()) => {},
        Err(err) if is_bind_mount_or_cross_device_rename_error(&err) => {
            let _unused = fs::remove_file(&tmp_path);
            warn!(
                target: LOG_TARGET,
                "Atomic rename onto identity file '{}' failed ({err}); falling back to non-atomic in-place write",
                target_path.display()
            );
            return write_identity_file_in_place(&target_path, contents, mode);
        },
        Err(err) => {
            let _unused = fs::remove_file(&tmp_path);
            return Err(err);
        },
    }

    let parent_dir = match target_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let dir = fs::File::open(parent_dir)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(target_family = "windows")]
fn write_identity_file<P: AsRef<Path>>(path: P, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let target_path = resolve_identity_write_target(path.as_ref());
    let tmp_path = temp_identity_path(&target_path)?;
    if fs::symlink_metadata(&tmp_path).is_ok_and(|m| m.file_type().is_file()) {
        let _unused = fs::remove_file(&tmp_path);
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
    let write_res = (|| -> io::Result<()> {
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    if let Err(e) = write_res {
        let _unused = fs::remove_file(&tmp_path);
        return Err(e);
    }
    fs::rename(&tmp_path, &target_path)
}

/// Check that the given path exists, is a file and has the correct file permissions (mac/linux only)
fn check_identity_file<P: AsRef<Path>>(path: P) -> Result<(), IdentityError> {
    if !path.as_ref().exists() {
        return Err(IdentityError::NotFound);
    }

    if !path.as_ref().metadata()?.is_file() {
        return Err(IdentityError::NotFile);
    }

    if !has_permissions(&path, REQUIRED_IDENTITY_PERMS)? && !has_permissions(&path, READONLY_IDENTITY_PERMS)? {
        return Err(IdentityError::InvalidPermissions);
    }
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
    #[error(
        "Identity file '{}' is read-only: a signature or address change could not be persisted",
        path.display()
    )]
    ReadOnly { path: PathBuf },
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
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("identity_management_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir.join("identity.json")
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
        let dir = std::env::temp_dir().join("identity_management_test_truncated_backup");
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
        let reloaded = load_node_identity(&path, TransportType::Tcp).unwrap();
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
        let dir = std::env::temp_dir().join("identity_management_test_dir_at_path");
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
        let dir = std::env::temp_dir().join("identity_management_test_missing_creates_new");
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
        let loaded = load_node_identity(&path, TransportType::Tcp).unwrap();
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
        let dir = std::env::temp_dir().join("identity_management_test_unwritable_parent");
        let _unused = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
        let dir = std::env::temp_dir().join("identity_management_test_same_second_backups");
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
        let dir = std::env::temp_dir().join("identity_management_test_eof_stdin_split");
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
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
            load_node_identity(&path, TransportType::Tcp).unwrap().node_id(),
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
    fn load_signed_identity_performs_no_write_preserving_mtime_and_inode() {
        use std::{
            os::unix::fs::MetadataExt,
            time::{Duration, SystemTime},
        };

        let path = temp_path("no_write_on_signed_load");
        let _unused = fs::remove_file(&path);

        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18189".parse().unwrap();
        let identity =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), vec![addr], PeerFeatures::COMMUNICATION_NODE);
        assert!(identity.is_signed());
        save_as_json(&path, &identity).unwrap();

        // Set mtime explicitly in the past so any rewrite is guaranteed to advance it.
        let past = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(past)).unwrap();
        drop(file);

        let before_meta = fs::metadata(&path).unwrap();
        let before_ino = before_meta.ino();
        let before_mtime = before_meta.modified().unwrap();

        let loaded = load_node_identity(&path, TransportType::Tcp).unwrap();
        assert_eq!(loaded.node_id(), identity.node_id());

        let after_meta = fs::metadata(&path).unwrap();
        assert_eq!(after_meta.ino(), before_ino, "inode must not change on read-only load");
        assert_eq!(
            after_meta.modified().unwrap(),
            before_mtime,
            "mtime must not change when loading an already-signed identity"
        );
    }

    #[test]
    fn load_unsigned_identity_adds_signature_via_temp_file_and_rename() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("unsigned_adds_sig");
        let _unused = fs::remove_file(&path);
        let tmp_path = temp_identity_path(&path).unwrap();
        let _unused = fs::remove_file(&tmp_path);

        let base =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        let unsigned = NodeIdentity::with_signature_unchecked(
            base.secret_key().clone(),
            Vec::new(),
            PeerFeatures::COMMUNICATION_NODE,
            None,
        );
        assert!(!unsigned.is_signed());
        save_as_json(&path, &unsigned).unwrap();

        let before_ino = fs::metadata(&path).unwrap().ino();

        let loaded = load_node_identity(&path, TransportType::Tcp).unwrap();
        assert!(loaded.is_signed());
        assert!(!tmp_path.exists(), "no .tmp file may remain after atomic save");

        let after_ino = fs::metadata(&path).unwrap().ino();
        assert_ne!(
            after_ino, before_ino,
            "atomic write via temp file and rename must replace the target inode"
        );
    }

    #[test]
    fn interrupted_write_before_rename_leaves_original_file_intact() {
        let path = temp_path("interrupted_write");
        let _unused = fs::remove_file(&path);
        let tmp_path = temp_identity_path(&path).unwrap();
        let _unused = fs::remove_file(&tmp_path);

        let original = b"// original valid identity content\n{\"intact\":true}\n";
        fs::write(&path, original).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let err = write_identity_file_with_hook(&path, b"new content that crashed mid-flight", |tmp| {
            assert!(tmp.exists(), "temp file must exist before rename");
            Err(io::Error::other("simulated process kill before rename"))
        })
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);

        // Clean up the left-over temp file from the simulated crash and verify the target file is untouched.
        let _unused = fs::remove_file(&tmp_path);
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "original identity file must remain intact if write fails before rename"
        );
    }

    #[test]
    fn readonly_0400_identity_needing_signature_returns_readonly_error_and_leaves_file_unchanged() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("readonly_0400_unsigned");
        if path.exists() {
            let _unused = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _unused = fs::remove_file(&path);
        }

        let base =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        let unsigned = NodeIdentity::with_signature_unchecked(
            base.secret_key().clone(),
            Vec::new(),
            PeerFeatures::COMMUNICATION_NODE,
            None,
        );
        save_as_json(&path, &unsigned).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let before_bytes = fs::read(&path).unwrap();
        let before_ino = fs::metadata(&path).unwrap().ino();

        let err =
            load_node_identity(&path, TransportType::Tcp).expect_err("0400 unsigned identity must fail with ReadOnly");
        match &err {
            IdentityError::ReadOnly { path: err_path } => assert_eq!(err_path, &path),
            other => panic!("expected IdentityError::ReadOnly, got: {other:?}"),
        }
        let msg = err.to_string();
        assert!(msg.contains(&path.display().to_string()), "error must name file: {msg}");
        assert!(
            msg.contains("signature or address change could not be persisted"),
            "unexpected ReadOnly message: {msg}"
        );

        assert_eq!(
            fs::read(&path).unwrap(),
            before_bytes,
            "0400 file bytes must not change"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().ino(),
            before_ino,
            "0400 file inode must not change"
        );

        // Restore write bit for test directory reuse.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn setup_node_identity_read_only_performs_no_write_on_readonly_file() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("peer_sync_readonly");
        if path.exists() {
            let _unused = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _unused = fs::remove_file(&path);
        }

        // Even an unsigned 0400 identity loads cleanly in read-only mode without writing to disk.
        let base =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        let unsigned = NodeIdentity::with_signature_unchecked(
            base.secret_key().clone(),
            Vec::new(),
            PeerFeatures::COMMUNICATION_NODE,
            None,
        );
        save_as_json(&path, &unsigned).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let before_bytes = fs::read(&path).unwrap();
        let before_ino = fs::metadata(&path).unwrap().ino();

        let loaded = setup_node_identity_read_only(
            &path,
            vec!["/ip4/127.0.0.1/tcp/18189".parse().unwrap()],
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .expect("read-only setup must succeed on 0400 file without writing");
        assert_eq!(loaded.node_id(), unsigned.node_id());

        assert_eq!(
            fs::read(&path).unwrap(),
            before_bytes,
            "read-only load must not modify file"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().ino(),
            before_ino,
            "read-only load must not change inode"
        );

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn loading_identity_with_onion_addresses_on_tcp_leaves_file_unchanged() {
        let path = temp_path("tcp_preserves_onion_on_disk");
        let _unused = fs::remove_file(&path);

        let onion_addr: Multiaddr = "/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:18141"
            .parse()
            .unwrap();
        let tcp_addr: Multiaddr = "/ip4/198.51.100.7/tcp/18189".parse().unwrap();
        let identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            vec![onion_addr.clone(), tcp_addr.clone()],
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &identity).unwrap();

        // Loading under TCP filters the onion address from the in-memory identity...
        let tcp_loaded = load_node_identity(&path, TransportType::Tcp).unwrap();
        assert_eq!(tcp_loaded.public_addresses(), vec![tcp_addr.clone()]);

        // ...but leaves the persisted file's onion address intact on disk.
        let disk_loaded = load_from_json::<_, NodeIdentity>(&path).unwrap().unwrap();
        assert_eq!(disk_loaded.public_addresses(), vec![onion_addr, tcp_addr]);
    }

    #[test]
    fn setup_node_identity_replaces_persisted_addresses_with_configured_addresses() {
        let path = temp_path("replace_persisted_addresses");
        let _unused = fs::remove_file(&path);

        let addr_a: Multiaddr = "/ip4/192.0.2.10/tcp/18189".parse().unwrap();
        let addr_b: Multiaddr = "/ip4/198.51.100.20/tcp/18189".parse().unwrap();

        let initial =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), vec![addr_a], PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&path, &initial).unwrap();

        let configured = setup_node_identity(
            &path,
            vec![addr_b.clone()],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();
        assert_eq!(
            configured.public_addresses(),
            vec![addr_b],
            "configured public_addresses must replace persisted addresses rather than append to them"
        );
    }

    #[test]
    fn leftover_temp_file_from_interrupted_save_does_not_block_subsequent_save() {
        let path = temp_path("stale_tmp_recovery");
        let _unused = fs::remove_file(&path);
        let tmp_path = temp_identity_path(&path).unwrap();
        let _unused = fs::remove_file(&tmp_path);

        let initial =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&path, &initial).unwrap();

        // Simulate a crash that leaves `<name>.tmp` behind on disk.
        let err = write_identity_file_with_hook(&path, b"partial crash data", |tmp| {
            assert!(tmp.exists());
            Err(io::Error::other("simulated crash before rename"))
        })
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(tmp_path.exists(), "simulated crash must leave .tmp behind");

        // A subsequent save must recover from the stale .tmp file and succeed.
        let updated =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&path, &updated).expect("save_as_json must recover when a stale .tmp file exists");
        assert!(
            !tmp_path.exists(),
            "stale .tmp file must be cleaned up by atomic rename"
        );

        let loaded = load_from_json::<_, NodeIdentity>(&path).unwrap().unwrap();
        assert_eq!(loaded.node_id(), updated.node_id());
    }

    #[test]
    fn save_as_json_on_unchanged_readonly_0400_identity_is_noop() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("readonly_0400_tor_noop");
        if path.exists() {
            let _unused = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _unused = fs::remove_file(&path);
        }

        let onion_addr: Multiaddr = "/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:18141"
            .parse()
            .unwrap();
        let identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            vec![onion_addr.clone()],
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &identity).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let before_bytes = fs::read(&path).unwrap();
        let before_ino = fs::metadata(&path).unwrap().ino();

        // Loading under Tor and re-saving the unchanged identity (as bootstrap does after comms init) must succeed on
        // 0400.
        let loaded = setup_node_identity(
            &path,
            vec![onion_addr],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tor,
        )
        .expect("Tor setup_node_identity on signed 0400 identity must succeed");
        save_as_json(&path, &*loaded).expect("re-saving unchanged identity to 0400 file must succeed as a no-op");

        assert_eq!(fs::read(&path).unwrap(), before_bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), before_ino);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn upgrade_tcp_node_with_empty_configured_addresses_preserves_persisted_address() {
        let path = temp_path("tcp_upgrade_empty_config");
        let _unused = fs::remove_file(&path);

        let persisted_addr: Multiaddr = "/ip4/1.2.3.4/tcp/18189".parse().unwrap();
        let initial = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            vec![persisted_addr.clone()],
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &initial).unwrap();

        let upgraded = setup_node_identity(
            &path,
            Vec::new(),
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
        )
        .unwrap();
        assert_eq!(
            upgraded.public_addresses(),
            vec![persisted_addr],
            "empty configured public_addresses must preserve persisted TCP address on upgrade"
        );
    }

    #[test]
    fn tor_bootstrap_sequence_on_readonly_0400_identity_succeeds_across_resign() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("tor_bootstrap_0400_resign");
        if path.exists() {
            let _unused = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _unused = fs::remove_file(&path);
        }

        let tcp_addr: Multiaddr = "/ip4/1.2.3.4/tcp/18189".parse().unwrap();
        let onion_addr: Multiaddr = "/onion3/bsmuof2cn4y2ysz253gzsvg3s72fcgh4f3qcm3hdlxdtcwe6al2dicyd:18141"
            .parse()
            .unwrap();
        // Store `[onion_addr, tcp_addr]` on disk so that replacing with `[tcp_addr]` and appending `onion_addr`
        // produces `[tcp_addr, onion_addr]` in memory, exercising order-independent address comparison.
        let identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            vec![onion_addr.clone(), tcp_addr.clone()],
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &identity).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();

        let before_bytes = fs::read(&path).unwrap();
        let before_ino = fs::metadata(&path).unwrap().ino();

        // Step 1: setup_node_identity replaces addresses with configured list (`[tcp_addr]`), dropping `onion_addr`
        let loaded = setup_node_identity(
            &path,
            vec![tcp_addr],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::TcpTor,
        )
        .expect("setup_node_identity on 0400 file must succeed");

        // Step 2: after_comms adds the onion address back and re-signs with a fresh timestamp
        loaded.add_public_address(onion_addr);
        loaded.sign();

        // Step 3: bootstrap re-saves the identity; because (public_key, features, public_addresses) match disk,
        // save_as_json treats it as unchanged and succeeds on the 0400 file.
        save_as_json(&path, loaded.as_ref())
            .expect("bootstrap save_as_json after add_public_address(onion) must succeed on 0400 file");

        assert_eq!(fs::read(&path).unwrap(), before_bytes);
        assert_eq!(fs::metadata(&path).unwrap().ino(), before_ino);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn symlinked_identity_file_updates_target_without_replacing_symlink() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join("identity_management_test_symlink_target");
        let _unused = fs::remove_dir_all(&dir);
        let target_dir = dir.join("real");
        let link_dir = dir.join("links");
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&link_dir).unwrap();

        let target_path = target_dir.join("base_node_id.json");
        let link_path = link_dir.join("base_node_id.json");

        let first =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&target_path, &first).unwrap();
        symlink(&target_path, &link_path).unwrap();

        let second =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&link_path, &second).unwrap();

        assert!(
            fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink(),
            "saving through a symlink must keep the symlink intact"
        );
        let loaded_from_target = load_from_json::<_, NodeIdentity>(&target_path).unwrap().unwrap();
        assert_eq!(loaded_from_target.node_id(), second.node_id());
    }

    #[test]
    fn ebusy_or_exdev_on_rename_falls_back_to_in_place_write_with_warning() {
        use std::os::unix::fs::MetadataExt;

        let path = temp_path("ebusy_in_place_fallback");
        let _unused = fs::remove_file(&path);
        let tmp_path = temp_identity_path(&path).unwrap();
        let _unused = fs::remove_file(&tmp_path);

        fs::write(&path, b"old content").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let before_ino = fs::metadata(&path).unwrap().ino();

        let new_bytes = b"// updated in place on single-file bind mount\n{\"updated\":true}\n";
        let (res, logs) = with_captured_logs(|| {
            write_identity_file_with_hooks(
                &path,
                new_bytes,
                |_| Ok(()),
                |_src, _dst| Err(io::Error::from_raw_os_error(16)), // EBUSY
            )
        });
        res.expect("EBUSY on rename must fall back to in-place write");

        assert!(
            !tmp_path.exists(),
            "temporary file must be cleaned up on EBUSY fallback"
        );
        assert_eq!(fs::read(&path).unwrap(), new_bytes);
        assert_eq!(
            fs::metadata(&path).unwrap().ino(),
            before_ino,
            "in-place fallback must preserve the existing inode of the bind-mounted file"
        );
        let warn_logs: Vec<&String> = logs
            .iter()
            .filter_map(|(lvl, msg)| (*lvl == Level::Warn).then_some(msg))
            .collect();
        assert_eq!(warn_logs.len(), 1, "expected one warning log on non-atomic fallback");
        assert!(warn_logs.first().is_some_and(|msg| msg.contains("falling back to non-atomic in-place write")));
    }
}

#[cfg(test)]
mod cross_platform_test {
    use super::*;

    #[test]
    fn atomic_save_replaces_existing_file_and_recovers_from_stale_tmp() {
        let dir = std::env::temp_dir().join(format!("identity_atomic_xplat_{}", std::process::id()));
        let _unused = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.json");
        let tmp_path = temp_identity_path(&path).unwrap();

        let first =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&path, &first).unwrap();

        // Leave a stale .tmp file and overwrite the existing target file via atomic rename
        fs::write(&tmp_path, b"stale partial temp file").unwrap();
        let second =
            NodeIdentity::random_multiple_addresses(&mut rand::rng(), Vec::new(), PeerFeatures::COMMUNICATION_NODE);
        save_as_json(&path, &second).unwrap();

        assert!(!tmp_path.exists(), "stale .tmp file must be removed");
        let loaded = load_from_json::<_, NodeIdentity>(&path).unwrap().unwrap();
        assert_eq!(loaded.node_id(), second.node_id());
    }
}
