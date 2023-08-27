//  Copyright 2021, The Taiji Project
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

use std::sync::Arc;

#[derive(Debug, Default)]
pub struct AtomicRefCounterGuard(Arc<()>);

impl AtomicRefCounterGuard {
    pub fn get(&self) -> usize {
        // Subtract one to account for the initial CounterGuard reference
        Arc::strong_count(&self.0) - 1
    }
}

#[derive(Debug, Clone, Default)]
pub struct AtomicRefCounter(Arc<AtomicRefCounterGuard>);

impl AtomicRefCounter {
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a new AtomicRefCounterGuard. Each counting as reference until it is dropped.
    pub fn new_guard(&self) -> AtomicRefCounterGuard {
        AtomicRefCounterGuard(Arc::clone(&self.0 .0))
    }

    /// Get the reference count
    pub fn get(&self) -> usize {
        self.0.get()
    }
}
