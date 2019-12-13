// Copyright 2019 The Tari Project
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

mod future;
mod lazy_service;

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Mutex,
};

pub use self::{future::ServiceHandlesFuture, lazy_service::LazyService};
pub(crate) use future::handle_notifier_pair;

/// This macro unlocks a Mutex or RwLock. If the lock is
/// poisoned (i.e. panic while unlocked) the last value
/// before the panic is used.
macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        match $e.$m() {
            Ok(lock) => lock,
            Err(poisoned) => {
                log::warn!(target: "service_framework", "Lock has been POISONED and will be silently recovered");
                poisoned.into_inner()
            },
        }
    };
    ($e:expr) => {
        acquire_lock!($e, lock)
    };
}

/// Simple collection for named handles
pub struct ServiceHandles {
    handles: Mutex<HashMap<TypeId, Box<dyn Any + Sync + Send>>>,
}

impl ServiceHandles {
    /// Create a new ServiceHandles
    pub fn new() -> Self {
        Self {
            handles: Default::default(),
        }
    }

    /// Register a handle
    pub fn register<H>(&self, handle: H)
    where H: Any + Send + Sync {
        acquire_lock!(self.handles).insert(TypeId::of::<H>(), Box::new(handle));
    }

    /// Get a handle from the given type (`TypeId`) and downcast it to a type `H`.
    /// If the item does not exist or the downcast fails, `None` is returned.
    pub fn get_handle<H>(&self) -> Option<H>
    where H: Clone + 'static {
        self.get_handle_by_type_id(&TypeId::of::<H>())
    }

    /// Get a ServiceHandle by name and downcast it to a type `H`. If the item
    /// does not exist or the downcast fails, `None` is returned.
    pub fn get_handle_by_type_id<H>(&self, type_id: &TypeId) -> Option<H>
    where H: Clone + 'static {
        acquire_lock!(self.handles)
            .get(type_id)
            .and_then(|b| b.downcast_ref::<H>())
            .map(Clone::clone)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn service_handles_insert_get() {
        #[derive(Clone)]
        struct TestHandle;
        let handles = ServiceHandles::new();
        handles.register(TestHandle);
        handles.get_handle::<TestHandle>().unwrap();
        assert!(handles.get_handle::<()>().is_none());
        assert!(handles.get_handle::<usize>().is_none());
    }
}
