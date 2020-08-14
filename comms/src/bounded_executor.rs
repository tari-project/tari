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

use crate::runtime::current;
use futures::future::Either;
use std::{future::Future, sync::Arc};
use tokio::{
    runtime,
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
    inner: runtime::Handle,
    semaphore: Arc<Semaphore>,
}

impl BoundedExecutor {
    pub fn new(executor: runtime::Handle, num_permits: usize) -> Self {
        Self {
            inner: executor,
            semaphore: Arc::new(Semaphore::new(num_permits)),
        }
    }

    pub fn from_current(num_permits: usize) -> Self {
        Self::new(current(), num_permits)
    }

    pub fn can_spawn(&self) -> bool {
        self.semaphore.available_permits() > 0
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
    /// # Examples
    ///
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tari_comms::bounded_executor::BoundedExecutor;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    /// let executor = BoundedExecutor::new(rt.handle().clone(), 1);
    ///
    /// // Spawn a future onto the runtime
    /// // NOTE: BoundedExecutor::spawn is an async function and therefore, must be polled/awaited for the task to be spawned
    /// let task1 = executor.spawn(async {
    ///     println!("now running on a worker thread");
    /// });
    /// // This will spawn after task1
    /// let task2 = executor.spawn(async {
    ///     println!("will always run after the first task");
    /// });
    ///
    /// rt.block_on(task1);
    /// rt.block_on(task2);
    /// # }
    /// ```
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
        let permit = self.semaphore.clone().acquire_owned().await;
        self.do_spawn(permit, future)
    }

    fn do_spawn<F>(&self, permit: OwnedSemaphorePermit, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(async move {
            let ret = future.await;
            // Task is finished, release the permit
            drop(permit);
            ret
        })
    }
}

pub struct OptionallyBoundedExecutor {
    inner: Either<runtime::Handle, BoundedExecutor>,
}

impl OptionallyBoundedExecutor {
    pub fn new(executor: runtime::Handle, num_permits: Option<usize>) -> Self {
        Self {
            inner: num_permits
                .map(|n| Either::Right(BoundedExecutor::new(executor.clone(), n)))
                .unwrap_or_else(|| Either::Left(executor)),
        }
    }

    pub fn from_current(num_permits: Option<usize>) -> Self {
        Self::new(current(), num_permits)
    }

    pub fn can_spawn(&self) -> bool {
        match &self.inner {
            Either::Left(_) => true,
            Either::Right(exec) => exec.can_spawn(),
        }
    }

    pub fn try_spawn<F>(&self, future: F) -> Result<JoinHandle<F::Output>, TrySpawnError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match &self.inner {
            Either::Left(exec) => Ok(exec.spawn(future)),
            Either::Right(exec) => exec.try_spawn(future),
        }
    }

    pub async fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match &self.inner {
            Either::Left(exec) => exec.spawn(future),
            Either::Right(exec) => exec.spawn(future).await,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::runtime;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };
    use tokio::time::delay_for;

    #[runtime::test_basic]
    async fn spawn() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_cloned = flag.clone();
        let executor = BoundedExecutor::new(runtime::current(), 1);

        // Spawn 1
        let task1_fut = executor
            .spawn(async move {
                delay_for(Duration::from_millis(1)).await;
                flag_cloned.store(true, Ordering::SeqCst);
            })
            .await;

        // Spawn 2
        let task2_fut = executor
            .spawn(async move {
                // This will panic if this task is spawned before task1 completes (e.g if num_permitted > 1)
                assert_eq!(flag.load(Ordering::SeqCst), true);
            })
            .await;

        task2_fut.await.unwrap();
        task1_fut.await.unwrap();
    }
}
