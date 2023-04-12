// Copyright 2022. The Tari Project
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

use crate::key_manager_service::{error::KeyManagerStorageError, storage::database::KeyManagerState};

/// This trait defines the required behaviour that a storage backend must provide for the Key Manager service.
pub trait KeyManagerBackend: Send + Sync + Clone {
    /// This will retrieve the key manager specified by the branch string, None is returned if the key manager is not
    /// found for the branch.
    fn get_key_manager(&self, branch: String) -> Result<Option<KeyManagerState>, KeyManagerStorageError>;
    /// This will add an additional branch for the key manager to track.
    fn add_key_manager(&self, key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError>;
    /// This will increase the key index of the specified branch, and returns an error if the branch does not exist.
    fn increment_key_index(&self, branch: String) -> Result<(), KeyManagerStorageError>;
    /// This method will set the currently stored key index for the key manager.
    fn set_key_index(&self, branch: String, index: u64) -> Result<(), KeyManagerStorageError>;
}
