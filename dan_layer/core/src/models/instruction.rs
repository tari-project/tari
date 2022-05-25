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

use std::fmt::{Display, Formatter};

use digest::Digest;
use tari_common_types::types::FixedHash;
use tari_crypto::common::Blake256;
use tari_utilities::hex::Hex;

use crate::models::TemplateId;

#[derive(Clone, Debug)]
pub struct Instruction {
    template_id: TemplateId,
    method: String,
    args: Vec<u8>,
    // from: TokenId,
    // signature: ComSig,
    hash: FixedHash,
}

impl PartialEq for Instruction {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl Instruction {
    pub fn new(
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
        /* from: TokenId,
         * _signature: ComSig, */
    ) -> Self {
        let mut s = Self {
            template_id,
            method,
            args,
            // from,
            // TODO: this is obviously wrong
            // signature: ComSig::default(),
            hash: FixedHash::zero(),
        };
        s.hash = s.calculate_hash();
        s
    }

    pub fn template_id(&self) -> TemplateId {
        self.template_id
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn args(&self) -> &[u8] {
        &self.args
    }

    // // TODO: rename to avoid use of from
    // pub fn from_owner(&self) -> &TokenId {
    //     &self.from
    // }

    // pub fn _signature(&self) -> &ComSig {
    //     &self.signature
    // }

    pub fn hash(&self) -> &FixedHash {
        &self.hash
    }

    pub fn calculate_hash(&self) -> FixedHash {
        let b = Blake256::new().chain(self.method.as_bytes()).chain(&self.args);
        // b.chain(self.from.as_bytes())
        //     .chain(com_sig_to_bytes(&self.signature))
        b.finalize().into()
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Method: {}, Hash: {}, Args: {} bytes, Template: {}",
            self.method,
            self.hash.to_hex(),
            self.args.len(),
            self.template_id
        )
    }
}
