// Copyright 2020. The Tari Project
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
//

use tari_core::transactions::transaction_components::Transaction;

#[derive(Default)]
pub struct TestBlockBuilder {}

#[allow(dead_code)]
impl TestBlockBuilder {
    pub fn new_block(&self, name: &str) -> TestBlockBuilderInner {
        TestBlockBuilderInner::new(name)
    }
}

#[derive(Debug, Clone)]
pub struct TestBlockBuilderInner {
    pub name: String,
    pub child_of: Option<String>,
    pub difficulty: Option<u64>,
    pub transactions: Vec<Transaction>,
}

#[allow(dead_code)]
impl TestBlockBuilderInner {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            child_of: None,
            difficulty: None,
            transactions: Vec::new(),
        }
    }

    pub fn child_of(mut self, parent: &str) -> Self {
        self.child_of = Some(parent.to_string());
        self
    }

    pub fn difficulty(mut self, difficulty: u64) -> Self {
        self.difficulty = Some(difficulty);
        self
    }

    pub fn with_transactions(mut self, transactions: Vec<Transaction>) -> Self {
        self.transactions = transactions;
        self
    }
}
