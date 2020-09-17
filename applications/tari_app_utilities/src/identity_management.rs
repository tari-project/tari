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

use crate::utilities::ExitCodes;
use log::*;
use rand::rngs::OsRng;
use std::{clone::Clone, fs, path::Path, string::ToString, sync::Arc};
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_core::transactions::types::PrivateKey;
use tari_crypto::{
    keys::SecretKey as SecretKeyTrait,
    tari_utilities::{hex::Hex, message_format::MessageFormat},
};

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
    public_address: &Multiaddr,
    create_id: bool,
    peer_features: PeerFeatures,
) -> Result<Arc<NodeIdentity>, ExitCodes>
{
    match load_identity(&identity_file) {
        Ok(id) => Ok(Arc::new(id)),
        Err(e) => {
            if !create_id {
                error!(
                    target: LOG_TARGET,
                    "Node identity information not found. {}. You can update the configuration file to point to a \
                     valid node identity file, or re-run the node with the --create-id flag to create a new identity.",
                    e
                );
                return Err(ExitCodes::ConfigError);
            }

            debug!(target: LOG_TARGET, "Node id not found. {}. Creating new ID", e);

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
                    Err(ExitCodes::ConfigError)
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

    let id_str = std::fs::read_to_string(path.as_ref()).map_err(|e| {
        format!(
            "The node identity file, {}, could not be read. {}",
            path.as_ref().to_str().unwrap_or("?"),
            e.to_string()
        )
    })?;
    let id = NodeIdentity::from_json(&id_str).map_err(|e| {
        format!(
            "The node identity file, {}, has an error. {}",
            path.as_ref().to_str().unwrap_or("?"),
            e.to_string()
        )
    })?;
    info!(
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
    public_addr: Multiaddr,
    features: PeerFeatures,
) -> Result<NodeIdentity, String>
{
    let private_key = PrivateKey::random(&mut OsRng);
    let node_identity = NodeIdentity::new(private_key, public_addr, features)
        .map_err(|e| format!("We were unable to construct a node identity. {}", e.to_string()))?;
    save_as_json(path, &node_identity)?;
    Ok(node_identity)
}

/// Loads the node identity from json at the given path
/// ## Parameters
/// `path` - Path to file from which to load the node identity
///
/// ## Returns
/// Result containing an object on success, string will indicate reason on error
pub fn load_from_json<P: AsRef<Path>, T: MessageFormat>(path: P) -> Result<T, String> {
    if !path.as_ref().exists() {
        return Err(format!(
            "Identity file, {}, does not exist.",
            path.as_ref().to_str().unwrap()
        ));
    }

    let contents = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let object = T::from_json(&contents).map_err(|err| err.to_string())?;
    Ok(object)
}

/// Saves the node identity as json at a given path, creating it if it does not already exist
/// ## Parameters
/// `path` - Path to save the file
/// `object` - Data to be saved
///
/// ## Returns
/// Result to check if successful or not, string will indicate reason on error
pub fn save_as_json<P: AsRef<Path>, T: MessageFormat>(path: P, object: &T) -> Result<(), String> {
    let json = object.to_json().unwrap();
    if let Some(p) = path.as_ref().parent() {
        if !p.exists() {
            fs::create_dir_all(p).map_err(|e| format!("Could not save json to data folder. {}", e.to_string()))?;
        }
    }
    fs::write(path.as_ref(), json.as_bytes()).map_err(|e| {
        format!(
            "Error writing json file, {}. {}",
            path.as_ref().to_str().unwrap_or("<invalid UTF-8>"),
            e.to_string()
        )
    })?;

    Ok(())
}
