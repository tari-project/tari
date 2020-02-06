// Copyright 2019. The Tari Project
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

use crate::{
    backend::{ArrayLike, ArrayLikeExt},
    error::MerkleMountainRangeError,
};
use std::{
    cmp::min,
    sync::{Arc, RwLock},
};

/// MemBackendVec is a shareable, memory only, vector that can be be used with MmrCache to store checkpoints.
#[derive(Debug, Clone)]
pub struct MemBackendVec<T> {
    db: Arc<RwLock<Vec<T>>>,
}

impl<T> MemBackendVec<T> {
    pub fn new() -> Self {
        Self {
            db: Arc::new(RwLock::new(Vec::<T>::new())),
        }
    }
}

impl<T: Clone> ArrayLike for MemBackendVec<T> {
    type Error = MerkleMountainRangeError;
    type Value = T;

    fn len(&self) -> Result<usize, Self::Error> {
        Ok(self
            .db
            .read()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .len())
    }

    fn push(&mut self, item: Self::Value) -> Result<usize, Self::Error> {
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .push(item);
        Ok(self.len()? - 1)
    }

    fn get(&self, index: usize) -> Result<Option<Self::Value>, Self::Error> {
        Ok(self
            .db
            .read()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .get(index)
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?)
    }

    fn get_or_panic(&self, index: usize) -> Self::Value {
        self.db.read().unwrap()[index].clone()
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .clear();
        Ok(())
    }
}

impl<T: Clone> ArrayLikeExt for MemBackendVec<T> {
    type Value = T;

    fn truncate(&mut self, len: usize) -> Result<(), MerkleMountainRangeError> {
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .truncate(len);
        Ok(())
    }

    fn shift(&mut self, n: usize) -> Result<(), MerkleMountainRangeError> {
        let drain_n = min(
            n,
            self.len()
                .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?,
        );
        self.db
            .write()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .drain(0..drain_n);
        Ok(())
    }

    fn for_each<F>(&self, f: F) -> Result<(), MerkleMountainRangeError>
    where F: FnMut(Result<Self::Value, MerkleMountainRangeError>) {
        self.db
            .read()
            .map_err(|e| MerkleMountainRangeError::BackendError(e.to_string()))?
            .iter()
            .map(|v| Ok(v.clone()))
            .for_each(f);
        Ok(())
    }
}
