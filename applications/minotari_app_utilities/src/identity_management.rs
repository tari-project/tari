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
    peer_manager::{NodeId, PeerFeatures},
    tor::TorIdentity,
    types::CommsPublicKey,
};
use tari_p2p::TransportType;
use tari_utilities::hex::Hex;
use zeroize::Zeroizing;

pub const LOG_TARGET: &str = "minotari_application";

const REQUIRED_IDENTITY_PERMS: u32 = 0o100600;
/// Also accepted on load: an owner-only file the operator has made read-only so it is never rewritten.
const READ_ONLY_IDENTITY_PERMS: u32 = 0o100400;

/// Loads the node identity, or creates a new one if create_id is true
///
/// ## Parameters
/// - `identity_file` - Reference to file path
/// - `public_address` - Network address of the base node
/// - `create_id` - Only applies if the identity_file does not exist or is malformed. If true, a new identity will be
///   created, otherwise the user will be prompted to create a new ID
/// - `peer_features` - Enables features of the base node
/// - `persist` - If false the identity file is never written: a missing or unreadable identity is an error, and a
///   signature or address change is kept in memory only. Lets another process share a running node's identity file.
///
/// # Return
/// A NodeIdentity wrapped in an atomic reference counter on success, the exit code indicating the reason on failure
pub fn setup_node_identity<P: AsRef<Path>>(
    identity_file: P,
    public_addresses: Vec<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
    transport_type: TransportType,
    persist: bool,
) -> Result<Arc<NodeIdentity>, ExitError> {
    setup_node_identity_with_prompts(
        identity_file,
        public_addresses,
        create_id,
        peer_features,
        transport_type,
        persist,
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
    persist: bool,
    prompt_missing: FMissing,
    prompt_corrupt: FCorrupt,
) -> Result<Arc<NodeIdentity>, ExitError>
where
    P: AsRef<Path>,
    FMissing: FnOnce(&str) -> bool,
    FCorrupt: FnOnce(&str) -> bool,
{
    match load_node_identity(&identity_file, transport_type, persist) {
        Ok(mut id) => {
            id.set_peer_features(peer_features);
            // Filter and deduplicate addresses based on transport type
            let mut filtered_addresses = Vec::new();
            for addr in filter_addresses_for_transport(public_addresses, transport_type) {
                if !filtered_addresses.contains(&addr) {
                    filtered_addresses.push(addr);
                }
            }
            let persisted_addresses = id.public_addresses();
            if transport_type.uses_tor_hidden_service() {
                for addr in &persisted_addresses {
                    if addr.iter().any(|p| matches!(p, Protocol::Onion3(_))) && !filtered_addresses.contains(addr) {
                        filtered_addresses.push(addr.clone());
                    }
                }
            }
            let dropped: Vec<&Multiaddr> = persisted_addresses
                .iter()
                .filter(|addr| !filtered_addresses.contains(addr))
                .collect();
            if !dropped.is_empty() {
                info!(
                    target: LOG_TARGET,
                    "Dropping persisted public addresses not present in configured addresses: {dropped:?}"
                );
            }
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
        Err(e @ (IdentityError::ReadOnly(_) | IdentityError::PersistFailed { .. })) => {
            error!(target: LOG_TARGET, "{e}");
            Err(ExitError::new(ExitCode::ConfigError, e))
        },
        Err(e @ (IdentityError::NotFound | IdentityError::Malformed { .. } | IdentityError::Io(_))) if !persist => {
            let msg = format!(
                "Node identity file '{}' could not be loaded ({e}) and will not be created or replaced by this \
                 process.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(e @ (IdentityError::Malformed { .. } | IdentityError::Io(_)))
            if identity_file
                .as_ref()
                .metadata()
                .is_ok_and(|m| m.permissions().readonly()) =>
        {
            let msg = format!(
                "Node identity file '{}' is read-only and could not be loaded ({e}); it will not be replaced.",
                identity_file.as_ref().display()
            );
            error!(target: LOG_TARGET, "{msg}");
            Err(ExitError::new(ExitCode::ConfigError, msg))
        },
        Err(e @ (IdentityError::PublicKeyMismatch | IdentityError::NodeIdMismatch)) => {
            let msg = format!(
                "Node identity file '{}' is inconsistent ({e}); refusing to replace the existing secret key.",
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
    persist: bool,
) -> Result<NodeIdentity, IdentityError> {
    check_identity_file(&path)?;

    let id_str = Zeroizing::new(fs::read_to_string(path.as_ref())?);
    let id = json5::from_str::<NodeIdentity>(&id_str)?;

    let expected_public_key = CommsPublicKey::from_secret_key(id.secret_key());
    if id.public_key() != &expected_public_key {
        return Err(IdentityError::PublicKeyMismatch);
    }

    let expected_node_id = NodeId::from_key(&expected_public_key);
    if id.node_id() != &expected_node_id {
        return Err(IdentityError::NodeIdMismatch);
    }

    let signature_is_valid = id.identity_signature_read().as_ref().is_some_and(|signature| {
        signature
            .verify(id.public_key(), id.features(), &id.public_addresses())
            .is_ok()
    });
    // Only a missing or invalid signature is written back, so an unchanged file is never rewritten on the read path.
    let changed = !signature_is_valid;
    if !signature_is_valid {
        warn!(
            target: LOG_TARGET,
            "Node identity file '{}' has a missing or invalid signature; replacing the signature while preserving the secret key",
            path.as_ref().display()
        );
        id.sign();
    }
    if persist && changed {
        // Never let a save failure reach the caller as a load error, which would back up and replace a good identity.
        save_as_json(&path, &id).map_err(|e| {
            let path = path.as_ref().to_path_buf();
            let source = match e {
                IdentityError::Io(err) => err,
                e => io::Error::other(e),
            };
            if source.kind() == io::ErrorKind::ReadOnlyFilesystem ||
                path.metadata().is_ok_and(|m| m.permissions().readonly())
            {
                IdentityError::ReadOnly(path)
            } else {
                IdentityError::PersistFailed { path, source }
            }
        })?;
    }

    let id = if transport_type == TransportType::Tcp {
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
        id
    } else {
        id
    };

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

    let contents = Zeroizing::new(fs::read_to_string(path)?);
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

/// Serializes an object to JSON5 wrapped in Zeroizing<String>
pub fn serialize_to_json<T: Serialize>(object: &T) -> Result<Zeroizing<String>, IdentityError> {
    let json = Zeroizing::new(json5::to_string(object)?);
    Ok(json)
}

/// Saves the identity as json at a given path with 0600 file permissions (UNIX-only), creating it if it does not
/// already exist. A file that already holds exactly these contents is left untouched, and a read-only file is never
/// replaced (`PermissionDenied`).
///
/// ## Parameters
/// `path` - Path to save the file
/// `object` - Data to be saved
///
/// ## Returns
/// Result to check if successful or not, string will indicate reason on error
pub fn save_as_json<P: AsRef<Path>, T: Serialize>(path: P, object: &T) -> Result<(), IdentityError> {
    let json = serialize_to_json(object)?;
    if let Some(p) = path.as_ref().parent() &&
        !p.exists()
    {
        fs::create_dir_all(p)?;
    }
    const COMMENT_HEADER: &[u8] =
        b"// This file is generated by the Minotari base node. Any changes will be overwritten.\n";
    if let Ok(existing) = fs::read(path.as_ref()).map(Zeroizing::new) &&
        existing.strip_prefix(COMMENT_HEADER) == Some(json.as_bytes())
    {
        return Ok(());
    }
    if path.as_ref().metadata().is_ok_and(|m| m.permissions().readonly()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is read-only", path.as_ref().display()),
        )
        .into());
    }
    write_identity_file(path.as_ref(), COMMENT_HEADER, json.as_bytes())?;
    Ok(())
}

/// Writes the identity file by writing and syncing `<name>.tmp` and renaming it over `path`, so a crash at any point
/// leaves either the old or the new file, never a partial one. Only mode 0600 and the owner carry over to the new
/// inode: SELinux/AppArmor labels, POSIX ACLs and other xattrs on the old file are lost, and an immutable
/// (`chattr +i`) file makes the rename fail.
#[cfg(target_family = "unix")]
fn write_identity_file<P: AsRef<Path>>(path: P, header: &[u8], contents: &[u8]) -> io::Result<()> {
    // Writers in this process (e.g. the node's Tor callback and bootstrap) share the fixed temp name, so take turns.
    // No cross-process lock: other processes (minotari_peer_sync) load without persisting.
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Write next to the real file so a symlinked identity is updated rather than replaced by a regular file.
    let path = match fs::canonicalize(path.as_ref()) {
        Ok(real) => real,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            // A dangling symlink (e.g. into a secrets dir that is populated on first start) must be followed too.
            let mut path = path.as_ref().to_path_buf();
            while fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                let target = fs::read_link(&path)?;
                path = path.parent().unwrap_or(Path::new("")).join(target);
            }
            path
        },
        Err(err) => return Err(err),
    };
    let tmp = write_identity_temp_file(&path, header, contents)?;
    if let Err(err) = fs::rename(&tmp, &path) {
        let _unused = fs::remove_file(&tmp);
        return Err(err);
    }
    // The new file is already in place, so failing to sync the directory must not be reported as a failed save.
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if let Err(err) = fs::File::open(dir).and_then(|d| d.sync_all()) {
        warn!(target: LOG_TARGET, "Could not sync directory '{}' after writing the identity file: {err}", dir.display());
    }
    Ok(())
}

/// Writes and syncs `<path>.tmp` with 0600 permissions and returns its path. The permissions are applied before any
/// contents are written, so the secret is never readable by other users.
#[cfg(target_family = "unix")]
fn write_identity_temp_file(path: &Path, header: &[u8], contents: &[u8]) -> io::Result<PathBuf> {
    use std::{
        io::Write,
        os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // A temp file left by a crashed write would make `create_new` fail forever; it is never the live identity.
    match fs::remove_file(&tmp) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
        _ => {},
    }
    // `mode()` and `chmod` only take the permission bits, so mask off the file type bits.
    let mode = REQUIRED_IDENTITY_PERMS & 0o7777;
    // `create_new` refuses to follow a symlink planted at the temp path.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)?;
    let result = (|| {
        // `mode()` is reduced by the umask, so set the exact permissions while the file is still empty.
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        // The rename installs a new inode, so keep the existing file's owner (e.g. when run via sudo) or the node
        // may no longer be able to read its own identity. The group is irrelevant at 0600 and an unprivileged owner
        // may not be allowed to set it, so only a different owner triggers the change.
        if let Ok(existing) = fs::metadata(path) &&
            existing.uid() != file.metadata()?.uid()
        {
            std::os::unix::fs::fchown(&file, Some(existing.uid()), Some(existing.gid()))?;
        }
        file.write_all(header)?;
        file.write_all(contents)?;
        file.sync_all()
    })();
    if let Err(err) = result {
        let _unused = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(tmp)
}

#[cfg(target_family = "windows")]
fn write_identity_file<P: AsRef<Path>>(path: P, header: &[u8], contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::File::create(path)?;
    file.write_all(header)?;
    file.write_all(contents)?;
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

    if !has_permissions(&path, REQUIRED_IDENTITY_PERMS)? && !has_permissions(&path, READ_ONLY_IDENTITY_PERMS)? {
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
    #[error("Identity file was not found")]
    NotFound,
    #[error("Path is not a file")]
    NotFile,
    #[error("Malformed identity file at line {line}, column {column}")]
    Malformed { line: usize, column: usize },
    #[error("Public key does not match secret key")]
    PublicKeyMismatch,
    #[error("Node id does not match public key")]
    NodeIdMismatch,
    #[error(
        "Identity file '{}' is read-only; a signature or address change could not be persisted",
        .0.display()
    )]
    ReadOnly(PathBuf),
    #[error("Could not persist identity file '{}': {source}", path.display())]
    PersistFailed { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<json5::Error> for IdentityError {
    fn from(err: json5::Error) -> Self {
        if let Some(pos) = err.position() {
            IdentityError::Malformed {
                line: pos.line.saturating_add(1),
                column: pos.column.saturating_add(1),
            }
        } else {
            IdentityError::Malformed { line: 0, column: 0 }
        }
    }
}

#[cfg(all(test, target_family = "unix"))]
mod test {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use tari_crypto::keys::SecretKey;

    use super::*;

    fn temp_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.json");
        (dir, path)
    }

    #[test]
    fn it_creates_the_identity_file_with_the_required_permissions() {
        let (_dir, path) = temp_path();
        let _unused = fs::remove_file(&path);

        save_as_json(&path, &"identity").unwrap();

        assert!(has_permissions(&path, REQUIRED_IDENTITY_PERMS).unwrap());
        check_identity_file(&path).unwrap();
    }

    #[test]
    fn it_tightens_the_permissions_of_an_existing_file_before_writing() {
        let (_dir, path) = temp_path();
        fs::write(&path, b"old contents").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        save_as_json(&path, &"identity").unwrap();

        assert!(has_permissions(&path, REQUIRED_IDENTITY_PERMS).unwrap());
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.ends_with("\"identity\""), "unexpected contents: {contents}");
    }

    #[test]
    fn malformed_identity_does_not_leak_secret_key() {
        let (_dir, path) = temp_path();
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
        let (_dir, path) = temp_path();

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
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
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
                true,
            )
        });
        let new_id = res.expect("setup_node_identity should back up and create a new identity");

        // Find the .bak file in `dir`
        let bak_files: Vec<PathBuf> = fs::read_dir(dir)
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
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let identity_dir = dir.join("identity.json");
        fs::create_dir_all(&identity_dir).unwrap();

        let err = setup_node_identity(
            &identity_dir,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert!(
            identity_dir.is_dir(),
            "directory at identity path must not be moved or replaced"
        );

        let entries: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        assert_eq!(entries, vec![identity_dir]);
    }

    #[test]
    fn missing_identity_file_creates_new_without_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("identity.json");

        let id = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .expect("missing identity file should create a new one");
        assert!(path.is_file());
        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded.node_id(), id.node_id());

        let bak_files: Vec<PathBuf> = fs::read_dir(dir)
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
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
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
        fs::set_permissions(dir, fs::Permissions::from_mode(0o500)).unwrap();

        let res = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        );

        // Restore parent directory permissions before asserting so cleanup always succeeds.
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();

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
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
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
            true,
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
            true,
        )
        .expect("second backup and recreate should succeed");

        let mut bak_contents: Vec<Vec<u8>> = fs::read_dir(dir)
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
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("identity.json");

        // Missing file + create_id == false + EOF on stdin (simulated via prompt_default_yes_on_eof -> true)
        // creates the identity on first start without a terminal.
        let created = setup_node_identity_with_prompts(
            &path,
            Vec::new(),
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
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
            true,
            |_| true,
            |_| false,
        )
        .expect_err("corrupt identity with EOF-means-no must abort");
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert_eq!(fs::read(&path).unwrap(), corrupt_bytes);
    }

    fn fresh_identity_file(signed: bool) -> (tempfile::TempDir, PathBuf, NodeIdentity) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("identity.json");
        let id = NodeIdentity::with_signature_unchecked(
            tari_comms::types::CommsSecretKey::random(&mut rand::rng()),
            vec!["/ip4/127.0.0.1/tcp/18141".parse().unwrap()],
            PeerFeatures::COMMUNICATION_NODE,
            None,
        );
        if signed {
            id.sign();
        }
        save_as_json(&path, &id).unwrap();
        (tmp, path, id)
    }

    fn mtime_and_inode(path: &Path) -> (SystemTime, u64) {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::metadata(path).unwrap();
        (meta.modified().unwrap(), meta.ino())
    }

    fn tmp_path(path: &Path) -> PathBuf {
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        PathBuf::from(tmp)
    }

    #[test]
    fn loading_an_unchanged_signed_identity_does_not_write() {
        let (_dir, path, _id) = fresh_identity_file(true);
        let before = mtime_and_inode(&path);

        load_node_identity(&path, TransportType::Tcp, true).unwrap();

        assert_eq!(mtime_and_inode(&path), before);
    }

    #[test]
    fn loading_an_unsigned_identity_writes_via_temp_file_and_rename() {
        let (_dir, path, id) = fresh_identity_file(false);
        let (_, inode_before) = mtime_and_inode(&path);

        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();

        assert!(loaded.is_signed());
        assert_eq!(loaded.public_key(), id.public_key());
        assert_ne!(
            mtime_and_inode(&path).1,
            inode_before,
            "file must be replaced by rename"
        );
        assert!(!tmp_path(&path).exists(), "no temp file may remain");
        assert!(has_permissions(&path, REQUIRED_IDENTITY_PERMS).unwrap());
    }

    #[test]
    fn crash_before_rename_leaves_original_and_stale_temp_is_replaced() {
        let (_dir, path, _id) = fresh_identity_file(false);
        let original = fs::read(&path).unwrap();

        // Simulates dying after the temp file is written and synced, before the rename.
        write_identity_temp_file(&path, b"", b"partial").unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(tmp_path(&path).exists());

        // The next write must not be blocked by the stale temp file.
        load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert!(!tmp_path(&path).exists());
        assert_ne!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn read_only_identity_needing_a_signature_is_not_modified() {
        let (_dir, path, _id) = fresh_identity_file(false);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let original = fs::read(&path).unwrap();
        let before = mtime_and_inode(&path);

        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();

        assert!(matches!(&err, IdentityError::ReadOnly(p) if p == &path), "got {err:?}");
        assert!(err.to_string().contains(&path.display().to_string()));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(mtime_and_inode(&path), before);
        assert!(!tmp_path(&path).exists());
    }

    #[test]
    fn non_persisting_setup_never_writes_the_identity_file() {
        let (_dir, path, id) = fresh_identity_file(false);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let original = fs::read(&path).unwrap();
        let before = mtime_and_inode(&path);

        let loaded = setup_node_identity(
            &path,
            vec!["/ip4/127.0.0.2/tcp/18141".parse().unwrap()],
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            false,
        )
        .unwrap();

        assert_eq!(loaded.public_key(), id.public_key());
        assert!(loaded.is_signed());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(mtime_and_inode(&path), before);

        // A missing identity is an error rather than being created.
        let dir = path.parent().unwrap().to_path_buf();
        let missing = dir.join("missing.json");
        let err = setup_node_identity(
            &missing,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            false,
        )
        .unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert!(!missing.exists());
    }

    #[test]
    fn failed_save_during_load_is_fatal_and_never_replaces_the_identity() {
        let (dir, path, _id) = fresh_identity_file(false);
        let original = fs::read(&path).unwrap();
        // A directory at the temp path makes the save fail with an error other than a read-only target.
        fs::create_dir_all(tmp_path(&path)).unwrap();

        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::PersistFailed { .. }), "got {err:?}");

        let err = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert_eq!(fs::read(&path).unwrap(), original);
        let bak_files: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bak"))
            .collect();
        assert!(
            bak_files.is_empty(),
            "a good identity must not be backed up: {bak_files:?}"
        );
    }

    #[test]
    fn unwritable_directory_is_not_reported_as_read_only_file() {
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        if unsafe { geteuid() } == 0 {
            return;
        }
        let (dir, path, _id) = fresh_identity_file(false);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let res = load_node_identity(&path, TransportType::Tcp, true);

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let err = res.unwrap_err();
        assert!(matches!(err, IdentityError::PersistFailed { .. }), "got {err:?}");
    }

    #[test]
    fn read_only_signed_identity_with_different_formatting_loads_without_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("identity.json");
        let id = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let contents = format!(
            "// This file is generated by the Tari base node.\n\n{}\n",
            *serialize_to_json(&id).unwrap()
        );
        fs::write(&path, contents.as_bytes()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let before = mtime_and_inode(&path);

        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();

        assert_eq!(loaded.public_key(), id.public_key());
        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        assert_eq!(mtime_and_inode(&path), before);
    }

    #[test]
    fn symlinked_identity_is_updated_through_the_link() {
        let (dir, real, id) = fresh_identity_file(false);
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let loaded = load_node_identity(&link, TransportType::Tcp, true).unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(!tmp_path(&link).exists() && !tmp_path(&real).exists());
        let reloaded = load_node_identity(&real, TransportType::Tcp, false).unwrap();
        assert_eq!(reloaded.public_key(), id.public_key());
        assert_eq!(
            *reloaded.identity_signature_read(),
            *loaded.identity_signature_read(),
            "the new signature must be written to the real file"
        );
    }

    #[test]
    fn dangling_symlinked_identity_is_created_at_the_link_target() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("secrets")).unwrap();
        let link = tmp.path().join("identity.json");
        let real = tmp.path().join("secrets").join("identity.json");
        std::os::unix::fs::symlink("secrets/identity.json", &link).unwrap();

        let id = setup_node_identity(
            &link,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(has_permissions(&real, REQUIRED_IDENTITY_PERMS).unwrap());
        assert!(!tmp_path(&link).exists() && !tmp_path(&real).exists());
        assert_eq!(
            load_node_identity(&real, TransportType::Tcp, false)
                .unwrap()
                .public_key(),
            id.public_key()
        );
    }

    #[test]
    fn concurrent_saves_in_one_process_do_not_collide() {
        let (_dir, path) = temp_path();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..10 {
                        let id = NodeIdentity::random(
                            &mut rand::rng(),
                            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
                            PeerFeatures::COMMUNICATION_NODE,
                        );
                        save_as_json(&path, &id).unwrap();
                    }
                });
            }
        });
        load_node_identity(&path, TransportType::Tcp, false).unwrap();
        assert!(!tmp_path(&path).exists());
    }

    #[test]
    fn loading_identity_with_onion_addresses_under_tcp_leaves_file_unchanged() {
        let tcp: Multiaddr = "/ip4/127.0.0.1/tcp/18141".parse().unwrap();
        let onion: Multiaddr = "/onion3/pd6sf3mqkkkfrn4rk5odgcr2j5sn7m523a4tt7pzy5hrdvhafdgn7ydt:18141"
            .parse()
            .unwrap();
        let (_dir, path) = temp_path();
        let id = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            vec![tcp.clone(), onion.clone()],
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &id).unwrap();
        let original = fs::read(&path).unwrap();
        let before = mtime_and_inode(&path);

        let loaded_tcp = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded_tcp.public_addresses(), vec![tcp.clone()]);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(mtime_and_inode(&path), before);

        // A read-only (0400) signed identity with an onion address also loads under TCP without writing.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let loaded_ro = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded_ro.public_addresses(), vec![tcp.clone()]);
        assert_eq!(mtime_and_inode(&path), before);

        // A writable (0600) unsigned identity repairs its signature without stripping onion addresses.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let unsigned = NodeIdentity::with_signature_unchecked(
            tari_comms::types::CommsSecretKey::random(&mut rand::rng()),
            vec![tcp.clone(), onion.clone()],
            PeerFeatures::COMMUNICATION_NODE,
            None,
        );
        save_as_json(&path, &unsigned).unwrap();
        let repaired_tcp = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(repaired_tcp.public_addresses(), vec![tcp.clone()]);
        let reloaded = load_node_identity(&path, TransportType::Tor, false).unwrap();
        assert_eq!(reloaded.public_addresses(), vec![tcp.clone(), onion.clone()]);
        reloaded
            .identity_signature_read()
            .as_ref()
            .unwrap()
            .verify(reloaded.public_key(), reloaded.features(), &reloaded.public_addresses())
            .unwrap();

        // An unsigned 0400 identity loaded under TCP still fails with ReadOnly.
        save_as_json(&path, &unsigned).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let before_unsigned = mtime_and_inode(&path);
        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::ReadOnly(_)), "got {err:?}");
        assert_eq!(mtime_and_inode(&path), before_unsigned);
    }

    #[test]
    fn setup_node_identity_replaces_persisted_addresses_with_configured_set() {
        let (_dir, path) = temp_path();
        let addr_a: Multiaddr = "/ip4/127.0.0.1/tcp/18141".parse().unwrap();
        let addr_b: Multiaddr = "/ip4/127.0.0.1/tcp/18142".parse().unwrap();
        let onion: Multiaddr = "/onion3/pd6sf3mqkkkfrn4rk5odgcr2j5sn7m523a4tt7pzy5hrdvhafdgn7ydt:18141"
            .parse()
            .unwrap();
        let created = setup_node_identity(
            &path,
            vec![addr_a, onion.clone()],
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tor,
            true,
        )
        .unwrap();

        // Under Tor, persisted Onion3 addresses are preserved alongside configured addresses.
        let reloaded_tor = setup_node_identity(
            &path,
            vec![addr_b.clone()],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tor,
            true,
        )
        .unwrap();
        assert_eq!(reloaded_tor.node_id(), created.node_id());
        assert_eq!(reloaded_tor.public_addresses(), vec![addr_b.clone(), onion.clone()]);
        save_as_json(&path, &reloaded_tor).unwrap();
        let expected_sig = reloaded_tor.identity_signature_read().clone();

        // A read-only (0400) signed identity holding [tcp, onion] with configured [tcp] (including duplicates)
        // loads under TcpTor and TorTcp with the address list and signature unchanged.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        let before_ro = mtime_and_inode(&path);
        for transport in [TransportType::TcpTor, TransportType::TorTcp] {
            let loaded = setup_node_identity(
                &path,
                vec![addr_b.clone(), addr_b.clone()],
                false,
                PeerFeatures::COMMUNICATION_NODE,
                transport,
                true,
            )
            .unwrap();
            assert_eq!(loaded.public_addresses(), vec![addr_b.clone(), onion.clone()]);
            assert_eq!(*loaded.identity_signature_read(), expected_sig);
            assert_eq!(mtime_and_inode(&path), before_ro);
        }

        // Under TCP, only the configured TCP addresses remain in memory.
        let reloaded_tcp = setup_node_identity(
            &path,
            vec![addr_b.clone()],
            false,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .unwrap();
        assert_eq!(reloaded_tcp.node_id(), created.node_id());
        assert_eq!(reloaded_tcp.public_addresses(), vec![addr_b]);
    }

    #[test]
    fn read_only_corrupt_identity_is_not_replaced() {
        for corrupt in [b"// corrupt identity\n{broken".as_slice(), b"\xff\xfe{".as_slice()] {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("identity.json");
            fs::write(&path, corrupt).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
            let before = mtime_and_inode(&path);

            let err = setup_node_identity(
                &path,
                Vec::new(),
                true,
                PeerFeatures::COMMUNICATION_NODE,
                TransportType::Tcp,
                true,
            )
            .unwrap_err();

            assert!(matches!(err.exit_code, ExitCode::ConfigError));
            assert!(err.details.unwrap().contains("read-only"));
            assert_eq!(fs::read(&path).unwrap(), corrupt);
            assert_eq!(mtime_and_inode(&path), before);
            assert_eq!(
                fs::read_dir(tmp.path()).unwrap().count(),
                1,
                "no backup or temp file may be created"
            );
        }
    }

    #[test]
    fn it_serializes_to_zeroizing_string() {
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let serialized: Zeroizing<String> = serialize_to_json(&node_identity).unwrap();
        assert!(serialized.contains("public_key"));
    }

    #[test]
    fn public_key_mismatch_fails_to_load() {
        let (_dir, path) = temp_path();
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &node_identity).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let other_pk = CommsPublicKey::from_secret_key(&tari_comms::types::CommsSecretKey::random(&mut rand::rng()));
        let tampered = content.replace(&node_identity.public_key().to_hex(), &other_pk.to_hex());
        fs::write(&path, tampered.as_bytes()).unwrap();

        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::PublicKeyMismatch));

        let original_tampered_bytes = fs::read(&path).unwrap();
        let err = setup_node_identity(
            &path,
            Vec::new(),
            true,
            PeerFeatures::COMMUNICATION_NODE,
            TransportType::Tcp,
            true,
        )
        .unwrap_err();
        assert!(matches!(err.exit_code, ExitCode::ConfigError));
        assert_eq!(
            fs::read(&path).unwrap(),
            original_tampered_bytes,
            "an inconsistent identity must never be replaced, even when create_id is true"
        );
    }

    #[test]
    fn node_id_mismatch_fails_to_load() {
        let (_dir, path) = temp_path();
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        save_as_json(&path, &node_identity).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let other_pk = CommsPublicKey::from_secret_key(&tari_comms::types::CommsSecretKey::random(&mut rand::rng()));
        let other_node_id = NodeId::from_key(&other_pk);
        let tampered = content.replace(&node_identity.node_id().to_hex(), &other_node_id.to_hex());
        fs::write(&path, tampered.as_bytes()).unwrap();

        let err = load_node_identity(&path, TransportType::Tcp, true).unwrap_err();
        assert!(matches!(err, IdentityError::NodeIdMismatch));
    }

    #[test]
    fn invalid_stored_signature_is_replaced_without_changing_identity() {
        let (_dir, path) = temp_path();
        let node_identity = NodeIdentity::random(
            &mut rand::rng(),
            "/ip4/127.0.0.1/tcp/18141".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        node_identity.sign();
        save_as_json(&path, &node_identity).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let tampered = content.replace("18141", "18142");
        assert_ne!(content, tampered, "Tampered content should differ from original");
        fs::write(&path, tampered.as_bytes()).unwrap();

        let loaded = load_node_identity(&path, TransportType::Tcp, true).unwrap();
        assert_eq!(loaded.public_key(), node_identity.public_key());
        assert_eq!(loaded.node_id(), node_identity.node_id());
        assert_ne!(
            fs::read_to_string(&path).unwrap(),
            tampered,
            "the invalid signature must be replaced on disk"
        );
        loaded
            .identity_signature_read()
            .as_ref()
            .unwrap()
            .verify(loaded.public_key(), loaded.features(), &loaded.public_addresses())
            .unwrap();
    }
}
