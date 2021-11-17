//  Copyright 2021. The Tari Project
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

use crate::{
    models::{Instruction, QuorumCertificate, TreeNodeHash},
    storage::StorageError,
};

pub trait ChainUnitOfWork: Clone + Send + Sync {
    fn commit(&mut self) -> Result<(), StorageError>;
    fn add_node(&mut self, hash: TreeNodeHash, parent: TreeNodeHash, height: u32) -> Result<(), StorageError>;
    fn add_instruction(&mut self, node_hash: TreeNodeHash, instruction: Instruction) -> Result<(), StorageError>;
    fn get_locked_qc(&mut self) -> Result<QuorumCertificate, StorageError>;
    fn set_locked_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn get_prepare_qc(&mut self) -> Result<QuorumCertificate, StorageError>;
    fn set_prepare_qc(&mut self, qc: &QuorumCertificate) -> Result<(), StorageError>;
    fn commit_node(&mut self, node_hash: &TreeNodeHash) -> Result<(), StorageError>;
    // fn find_proposed_node(&mut self, node_hash: TreeNodeHash) -> Result<(Self::Id, UnitOfWorkTracker), StorageError>;
}
