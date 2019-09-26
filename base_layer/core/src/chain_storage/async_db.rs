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

use crate::chain_storage::{BlockchainBackend, BlockchainDatabase, ChainStorageError, ChainMetadata};
use tokio_executor::threadpool::blocking;
use crate::types::HashOutput;
use crate::transaction::TransactionKernel;
use std::future::Future;
use std::task::{Poll, Context};
use std::pin::Pin;

pub struct KernelQuery<T> where T: BlockchainBackend {
    hash: HashOutput,
    db: BlockchainDatabase<T>
}

impl<T> Future for KernelQuery<T> where T: BlockchainBackend {
    type Output = Result<TransactionKernel, ChainStorageError>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match blocking(|| self.db.fetch_kernel(self.hash.clone())) {
            Poll::Pending => Poll::Pending,
            // Map BlockingError -> ChainStorageError
            Poll::Ready(Err(_)) => Poll::Ready(Err(
                ChainStorageError::AccessError("Could not find a blocking thread to execute DB query".into()))),
            // Unwrap and lift ChainStorageError
            Poll::Ready(Ok(Err(e))) => Poll::Ready(Err(e)),
            // Unwrap and return result
            Poll::Ready(Ok(Ok(v))) => Poll::Ready(Ok(v)),
        }
    }
}

/// Returns the transaction kernel with the given hash.
pub fn fetch_kernel<T: BlockchainBackend>(db: BlockchainDatabase<T>, hash: HashOutput) -> KernelQuery<T>  {
    KernelQuery { hash, db: db.clone() }
}



