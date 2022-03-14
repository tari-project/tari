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

use std::str::FromStr;

use crate::{models::TreeNodeHash, storage::state::DbKeyValue};

#[derive(Debug)]
pub struct DbStateOpLogEntry {
    pub height: u64,
    pub merkle_root: Option<TreeNodeHash>,
    pub operation: DbStateOperation,
    pub schema: String,
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

impl DbStateOpLogEntry {
    pub fn set_operation(height: u64, key_value: DbKeyValue) -> Self {
        Self {
            height,
            merkle_root: None,
            operation: DbStateOperation::Set,
            schema: key_value.schema,
            key: key_value.key,
            value: Some(key_value.value),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DbStateOperation {
    Set,
    Delete,
}

impl DbStateOperation {
    pub fn as_op_str(&self) -> &str {
        use DbStateOperation::*;
        match self {
            Set => "S",
            Delete => "D",
        }
    }
}

impl FromStr for DbStateOperation {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use DbStateOperation::*;
        match s {
            "S" => Ok(Set),
            "D" => Ok(Delete),
            _ => Err(()),
        }
    }
}
