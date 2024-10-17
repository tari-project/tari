// Copyright 2020, The Tari Project
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

use std::{future::Future, sync::Arc};

use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

/// Error emitted from [`try_spawn`](self::BoundedExecutor::try_spawn) when there are no tasks available
#[derive(Debug)]
pub struct TrySpawnError;

/// A task executor bounded by a semaphore.
///
/// Use the asynchronous spawn method to spawn a task. If a given number of tasks are already spawned and have not
/// completed, the spawn function will block (asynchronously) until a previously spawned task completes.
pub struct BoundedExecutor {
    // inner: runtime::Handle,
    semaphore: Arc<Semaphore>,
}

impl BoundedExecutor {
    pub fn new(num_permits: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(num_permits)),
        }
    }

    pub fn allow_maximum() -> Self {
        Self::new(Self::max_theoretical_tasks())
    }

    pub const fn max_theoretical_tasks() -> usize {
        // Maximum from here: https://github.com/tokio-rs/tokio/blob/ce9eabfdd12a14efb74f5e6d507f2acbe7a814c9/tokio/src/sync/batch_semaphore.rs#L101
        // NOTE: usize::MAX >> 3 does not work. The reason is not clear, however 1152921504606846975 tasks seems
        // sufficient
        usize::MAX >> 4
    }

    pub fn can_spawn(&self) -> bool {
        self.num_available() > 0
    }

    /// Returns the remaining number of tasks that can be spawned on this executor without waiting.
    pub fn num_available(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn try_spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, TrySpawnError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let permit = self.semaphore.clone().try_acquire_owned().map_err(|_| TrySpawnError)?;
        let handle = self.do_spawn(permit, future);
        Ok(handle)
    }

    /// Spawn a future onto the Tokio runtime asynchronously blocking if there are too many
    /// spawned tasks.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// If the number of pending tasks exceeds the num_permits value given to `BoundedExecutor::new`
    /// the future returned from spawn will block until a permit is released.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub async fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        // SAFETY: acquire_owned only fails if the semaphore is closed (i.e self.semaphore.close() is called) - this
        // never happens in this implementation
        let permit = self.semaphore.clone().acquire_owned().await.expect("semaphore closed");
        self.do_spawn(permit, future)
    }

    fn do_spawn<F>(&self, permit: OwnedSemaphorePermit, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        // let task = task::Builder::new().inner
        tokio::spawn(async move {
            // Task is finished, release the permit
            let ret = future.await;
            drop(permit);
            ret
        })
    }
}

#[cfg(test)]
mod test {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };

    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn spawn() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_cloned = flag.clone();
        let executor = BoundedExecutor::new(1);

        // Spawn 1
        let task1_fut = executor
            .spawn(async move {
                sleep(Duration::from_millis(1)).await;
                flag_cloned.store(true, Ordering::SeqCst);
            })
            .await;

        // Spawn 2
        let task2_fut = executor
            .spawn(async move {
                // This will panic if this task is spawned before task1 completes (e.g if num_permitted > 1)
                assert!(flag.load(Ordering::SeqCst));
            })
            .await;

        task2_fut.await.unwrap();
        task1_fut.await.unwrap();
    }
}
