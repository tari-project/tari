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

use std::{clone::Clone, fs, path::Path, str::FromStr, string::ToString, sync::Arc};

use log::*;
use rand::rngs::OsRng;
use serde::{de::DeserializeOwned, Serialize};
use tari_common::{
    configuration::{bootstrap::prompt, utils::get_local_ip},
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_crypto::tari_utilities::hex::Hex;

pub const LOG_TARGET: &str = "tari_application";

/// Loads the node identity, or creates a new one if the --create-id flag was specified
/// ## Parameters
/// `identity_file` - Reference to file path
/// `public_address` - Network address of the base node
/// `create_id` - Whether an identity needs to be created or not
/// `peer_features` - Enables features of the base node
///
/// # Return
/// A NodeIdentity wrapped in an atomic reference counter on success, the exit code indicating the reason on failure
pub fn setup_node_identity<P: AsRef<Path>>(
    identity_file: P,
    public_address: &Option<Multiaddr>,
    create_id: bool,
    peer_features: PeerFeatures,
) -> Result<Arc<NodeIdentity>, ExitError> {
    match load_identity(&identity_file) {
        Ok(id) => match public_address {
            Some(public_address) => {
                id.set_public_address(public_address.clone());
                Ok(Arc::new(id))
            },
            None => Ok(Arc::new(id)),
        },
        Err(e) => {
            debug!(target: LOG_TARGET, "Failed to load node identity: {}", e);
            if !create_id {
                let prompt = prompt("Node identity does not exist.\nWould you like to to create one (Y/n)?");
                if !prompt {
                    error!(
                        target: LOG_TARGET,
                        "Node identity information not found. {}. You can update the configuration file to point to a \
                         valid node identity file, or re-run the node with the --create-id flag to create a new \
                         identity.",
                        e
                    );
                    return Err(ExitError::new(
                        ExitCode::ConfigError,
                        format!(
                            "Node identity information not found. {}. You can update the configuration file to point \
                             to a valid node identity file, or re-run the node with the --create-id flag to create a \
                             new identity.",
                            e
                        ),
                    ));
                };
            }

            debug!(target: LOG_TARGET, "Existing node id not found. {}. Creating new ID", e);

            match create_new_identity(&identity_file, public_address.clone(), peer_features) {
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
                    error!(target: LOG_TARGET, "Could not create new node id. {:?}.", e);
                    Err(ExitError::new(
                        ExitCode::ConfigError,
                        format!("Could not create new node id. {:?}.", e),
                    ))
                },
            }
        },
    }
}

/// Tries to construct a node identity by loading the secret key and other metadata from disk and calculating the
/// missing fields from that information.
/// ## Parameters
/// `path` - Reference to a path
///
/// ## Returns
/// Result containing a NodeIdentity on success, string indicates the reason on failure
pub fn load_identity<P: AsRef<Path>>(path: P) -> Result<NodeIdentity, String> {
    if !path.as_ref().exists() {
        return Err(format!(
            "Identity file, {}, does not exist.",
            path.as_ref().to_str().unwrap_or("?"),
        ));
    }

    let id_str = fs::read_to_string(path.as_ref()).map_err(|e| {
        format!(
            "The node identity file, {}, could not be read. {}",
            path.as_ref().to_str().unwrap_or("?"),
            e
        )
    })?;
    let id = json5::from_str::<NodeIdentity>(&id_str).map_err(|e| {
        format!(
            "The node identity file, {}, has an error. {}",
            path.as_ref().to_str().unwrap_or("?"),
            e
        )
    })?;
    // Check whether the previous version has a signature and sign if necessary
    if !id.is_signed() {
        id.sign();
    }
    debug!(
        "Node ID loaded with public key {} and Node id {}",
        id.public_key().to_hex(),
        id.node_id().to_hex()
    );
    Ok(id)
}
/// Create a new node id and save it to disk
/// ## Parameters
/// `path` - Reference to path to save the file
/// `public_addr` - Network address of the base node
/// `peer_features` - The features enabled for the base node
///
/// ## Returns
/// Result containing the node identity, string will indicate reason on error
pub fn create_new_identity<P: AsRef<Path>>(
    path: P,
    public_addr: Option<Multiaddr>,
    features: PeerFeatures,
) -> Result<NodeIdentity, String> {
    let node_identity = NodeIdentity::random(
        &mut OsRng,
        match public_addr {
            Some(public_addr) => public_addr,
            None => format!("{}/tcp/18141", get_local_ip().ok_or("Can't get local ip address")?)
                .parse()
                .map_err(|e: <Multiaddr as FromStr>::Err| e.to_string())?,
        },
        features,
    );
    save_as_json(path, &node_identity)?;
    Ok(node_identity)
}

/// Loads the node identity from json at the given path
/// ## Parameters
/// `path` - Path to file from which to load the node identity
///
/// ## Returns
/// Result containing an object on success, string will indicate reason on error
pub fn load_from_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T, String> {
    if !path.as_ref().exists() {
        return Err(format!(
            "Identity file, {}, does not exist.",
            path.as_ref().to_str().unwrap()
        ));
    }

    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let object = json5::from_str(&contents).map_err(|err| err.to_string())?;
    Ok(object)
}

/// Saves the node identity as json at a given path, creating it if it does not already exist
/// ## Parameters
/// `path` - Path to save the file
/// `object` - Data to be saved
///
/// ## Returns
/// Result to check if successful or not, string will indicate reason on error
pub fn save_as_json<P: AsRef<Path>, T: Serialize>(path: P, object: &T) -> Result<(), String> {
    let json = json5::to_string(object).map_err(|err| err.to_string())?;
    if let Some(p) = path.as_ref().parent() {
        if !p.exists() {
            fs::create_dir_all(p).map_err(|e| format!("Could not save json to data folder. {}", e))?;
        }
    }
    let json_with_comment = format!(
        "// This file is generated by the Tari base node. Any changes will be overwritten.\n{}",
        json
    );
    fs::write(path.as_ref(), json_with_comment.as_bytes()).map_err(|e| {
        format!(
            "Error writing json file, {}. {}",
            path.as_ref().to_str().unwrap_or("<invalid UTF-8>"),
            e
        )
    })?;

    Ok(())
}
