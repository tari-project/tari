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

use tari_common_types::types::PrivateKey;
use tari_key_manager::mnemonic::MnemonicLanguage;

use crate::key_manager_service::error::KeyManagerError;

#[async_trait::async_trait]
pub trait KeyManagerInterface: Clone + Send + Sync + 'static {
    async fn add_new_branch(&self, branch: String) -> Result<(), KeyManagerError>;

    async fn add_new_branches(&self, branches: Vec<String>) -> Result<(), KeyManagerError>;

    async fn get_next_key(&self, branch: String) -> Result<PrivateKey, KeyManagerError>;

    async fn get_key_at_index(&self, branch: String, index: u64) -> Result<PrivateKey, KeyManagerError>;

    async fn find_key_index(&self, branch: String, key: PrivateKey) -> Result<u64, KeyManagerError>;

    async fn update_current_key_index_if_higher(&self, branch: String, index: u64) -> Result<(), KeyManagerError>;

    async fn get_seed_words(&self, branch: String, language: &MnemonicLanguage)
        -> Result<Vec<String>, KeyManagerError>;
}
