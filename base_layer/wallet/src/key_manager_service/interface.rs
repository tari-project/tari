//  Copyright 2022, The Tari Project
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

use aes_gcm::Aes256Gcm;
use tari_common_types::types::PrivateKey;

use crate::key_manager_service::error::KeyManagerError;

#[derive(Debug, PartialEq)]
pub enum AddResult {
    NewEntry,
    AlreadyExists,
}

pub struct NextKeyResult {
    pub key: PrivateKey,
    pub index: u64,
}

#[async_trait::async_trait]
pub trait KeyManagerInterface: Clone + Send + Sync + 'static {
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerError>;

    async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), KeyManagerError>;

    async fn remove_encryption(&self) -> Result<(), KeyManagerError>;

    async fn get_next_key<T: Into<String> + Send>(&self, branch: T) -> Result<NextKeyResult, KeyManagerError>;

    async fn get_key_at_index<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<PrivateKey, KeyManagerError>;

    async fn find_key_index<T: Into<String> + Send>(&self, branch: T, key: &PrivateKey)
        -> Result<u64, KeyManagerError>;

    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerError>;
}
