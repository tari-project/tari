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

use crate::{models::TreeNodeHash, storage::chain::DbNode};

#[derive(Debug, Clone)]
pub struct Node {
    hash: TreeNodeHash,
    parent: TreeNodeHash,
    height: u32,
    is_committed: bool,
}

impl Node {
    pub fn new(hash: TreeNodeHash, parent: TreeNodeHash, height: u32, is_committed: bool) -> Node {
        Self {
            hash,
            parent,
            height,
            is_committed,
        }
    }

    pub fn hash(&self) -> &TreeNodeHash {
        &self.hash
    }

    pub fn parent(&self) -> &TreeNodeHash {
        &self.parent
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_committed(&self) -> bool {
        self.is_committed
    }
}

impl From<DbNode> for Node {
    fn from(db_node: DbNode) -> Self {
        Node::new(db_node.hash, db_node.parent, db_node.height, db_node.is_committed)
    }
}
