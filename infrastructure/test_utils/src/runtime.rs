// Copyright 2019, The Tari Project
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

use futures::FutureExt;
use std::{future::Future, pin::Pin};
use tokio::{runtime, runtime::Runtime, task};

pub fn create_runtime() -> Runtime {
    tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_io()
        .enable_time()
        .max_threads(4)
        .core_threads(1)
        .build()
        .expect("Could not create runtime")
}

/// Create a runtime and report if it panics. If there are tasks still running after the panic, this
/// will carry on running forever.
/// TODO: Create a test harness which allows test cleanup or at least fail the test after a timeout
pub fn test_async<F>(f: F)
where F: FnOnce(&mut TestRuntime) {
    let mut rt = TestRuntime::from(create_runtime());
    f(&mut rt);
    let handles = rt.handles.drain(..).collect::<Vec<_>>();
    for h in handles {
        rt.block_on(h);
    }
}

pub struct TestRuntime {
    inner: Runtime,
    handles: Vec<Pin<Box<dyn Future<Output = ()>>>>,
}

impl TestRuntime {
    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        self.inner.block_on(future)
    }

    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = self.inner.spawn(future);
        self.handles.push(
            async move {
                if let Err(err) = handle.await {
                    panic!("{:?}", err);
                }
            }
            .boxed(),
        );
    }

    pub fn spawn_unchecked<F>(&mut self, future: F) -> task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future)
    }

    pub fn handle(&self) -> &runtime::Handle {
        self.inner.handle()
    }
}

impl From<Runtime> for TestRuntime {
    fn from(rt: Runtime) -> Self {
        Self {
            inner: rt,
            handles: Vec::new(),
        }
    }
}

// TODO: WIP for a test harness which allows shutdown code to be run in the case of a successful/failed test
// pub fn run_with_complete<F, FComplete, TFut>(f: F, on_complete: FComplete)
// where F: FnOnce(&Runtime) {
//    let passed = Arc::new(AtomicBool::new(true));
//    let passed_inner = Arc::clone(&passed);
//    let (panic_tx, panic_rx) = oneshot::channel();
//    let (complete_tx, complete_rx) = oneshot::channel();
//    let panic_tx = Mutex::new(Some(panic_tx));
//    let complete_tx = Mutex::new(Some(complete_tx));
//    let rt = tokio::runtime::Builder::new()
//        .blocking_threads(4)
//        // Run the work scheduler on one thread so we can really see the effects of using `blocking` above
//        .core_threads(1)
//        .panic_handler(move |_| {
//            panic_tx.lock().unwrap().take().map(|tx| tx.send(()).unwrap());
//        })
//        .build()
//        .expect("Could not create runtime");
//
//    f(&rt, complete_tx);
//
//    rt.block_on(async move {
//        let did_panic = futures::future::select(panic_rx, complete_rx).await;
//        match did_panic {
//            Either::Right(_) => passed_inner.store(true, Ordering::SeqCst),
//            Either::Left(_) => passed_inner.store(false, Ordering::SeqCst),
//        };
//    });
//
//    assert!(passed.load(Ordering::SeqCst), "Panic occurred in spawned future");
//}
//
//#[cfg(test)]
// mod test {
//    use super::*;
//
//    #[test]
//    fn test_run_with_complete() {
//        let on_complete_called = Arc::new(AtomicBool::new(false));
//        let on_complete_called_inner = on_complete_called.clone();
//        run_with_complete(
//            |rt| {
//                rt.block_on(futures::future::ready(()));
//            },
//            || {
//                async move {
//                    on_complete_called_inner.store(true, Ordering::SeqCst);
//                }
//            },
//        );
//        assert_eq!(on_complete_called.load(Ordering::SeqCst), true);
//    }
//}
