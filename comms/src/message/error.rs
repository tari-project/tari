//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::peer_manager::node_id::NodeIdError;
use derive_error::Error;
use tari_crypto::signatures::SchnorrSignatureError;
use tari_utilities::{ciphers::cipher::CipherError, message_format::MessageFormatError};

#[derive(Error, Debug)]
pub enum MessageError {
    /// Multipart message is malformed
    MalformedMultipart,
    /// Failed to serialize message
    SerializeFailed,
    /// Failed to deserialize message
    DeserializeFailed,
    /// An error occurred serialising an object into binary
    BinarySerializeError,
    /// An error occurred deserialising binary data into an object
    BinaryDeserializeError,
    MessageFormatError(MessageFormatError),
    SchnorrSignatureError(SchnorrSignatureError),
    /// Failed to Encode or Decode the message using the Cipher
    CipherError(CipherError),
    NodeIdError(NodeIdError),
    /// Problem initializing the RNG
    RngError,
}
