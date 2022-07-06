// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
    RwLock,
    RwLockReadGuard,
    RwLockWriteGuard,
};

#[derive(Debug)]
pub struct UnitOfWorkTracker<TItem> {
    item: Arc<RwLock<TItem>>,
    is_dirty: Arc<AtomicBool>,
}

impl<TItem> Clone for UnitOfWorkTracker<TItem> {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            is_dirty: self.is_dirty.clone(),
        }
    }
}

impl<TItem> UnitOfWorkTracker<TItem> {
    pub fn new(item: TItem, is_dirty: bool) -> Self {
        Self {
            item: Arc::new(RwLock::new(item)),
            is_dirty: Arc::new(AtomicBool::new(is_dirty)),
        }
    }

    pub fn get(&self) -> RwLockReadGuard<TItem> {
        self.item.read().unwrap()
    }

    pub fn get_mut(&self) -> RwLockWriteGuard<TItem> {
        self.is_dirty.store(true, Ordering::SeqCst);
        self.item.write().unwrap()
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty.load(Ordering::SeqCst)
    }
}
