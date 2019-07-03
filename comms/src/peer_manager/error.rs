// Copyright 2019 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use crate::{connection::NetAddressError, peer_manager::node_id::NodeIdError, types::CommsDataStoreError};
use derive_error::Error;
use tari_storage::keyvalue_store::DatastoreError;
use tari_utilities::message_format::MessageFormatError;

#[derive(Debug, Error)]
pub enum PeerManagerError {
    /// The requested peer does not exist or could not be located
    PeerNotFoundError,
    /// The Thread Safety has been breached and the data access has become poisoned
    PoisonedAccess,
    /// Could not write or read from datastore
    DatastoreError(DatastoreError),
    /// A problem occurred during the serialization of the keys or data
    SerializationError(MessageFormatError),
    /// A problem occurred converting the serialized data into peers
    DeserializationError,
    /// The index doesn't relate to an existing peer
    IndexOutOfBounds,
    /// The requested operation can only be performed if the PeerManager is linked to a DataStore
    DatastoreUndefined,
    /// An empty response was received from the Datastore
    EmptyDatastoreQuery,
    /// The data update could not be performed
    DataUpdateError,
    /// The PeerManager doesn't have enough peers to fill the identity request
    InsufficientPeers,
    /// The peer has been banned
    BannedPeer,
    /// Problem initializing the RNG
    RngError,
    /// An problem has been encountered with the database
    DatabaseError(CommsDataStoreError),
    NodeIdError(NodeIdError),
    NetAddressError(NetAddressError),
}
