// Copyright 2021. The Tari Project
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

use tari_common_types::types::{Commitment, PublicKey};

#[derive(Clone)]
pub struct Token {
    name: String,
    output_status: String,
    asset_public_key: PublicKey,
    owner_commitment: Commitment,
    unique_id: Vec<u8>,
}

impl Token {
    pub fn new(
        name: String,
        output_status: String,
        asset_public_key: PublicKey,
        owner_commitment: Commitment,
        unique_id: Vec<u8>,
    ) -> Self {
        Self {
            name,
            output_status,
            asset_public_key,
            owner_commitment,
            unique_id,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn output_status(&self) -> &str {
        &self.output_status
    }

    pub fn asset_public_key(&self) -> &PublicKey {
        &self.asset_public_key
    }

    pub fn owner_commitment(&self) -> &Commitment {
        &self.owner_commitment
    }

    pub fn unique_id(&self) -> &[u8] {
        self.unique_id.as_slice()
    }
}
