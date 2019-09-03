use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct AtomicCounter {
    count: AtomicUsize,
}

impl AtomicCounter {
    pub fn new(c: usize) -> Self {
        AtomicCounter {
            count: AtomicUsize::new(c),
        }
    }

    #[inline]
    pub fn get(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    #[inline]
    pub fn set(&self, val: usize) {
        self.count.store(val, Ordering::Release);
    }

    #[inline]
    pub fn inc(&self) {
        self.count.fetch_add(1, Ordering::AcqRel);
    }

    #[inline]
    pub fn dec(&self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

impl fmt::Debug for AtomicCounter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AtomicCounter: {}", self.get())
    }
}

impl PartialEq for AtomicCounter {
    fn eq(&self, other: &AtomicCounter) -> bool {
        self.count.load(Ordering::Acquire) == other.count.load(Ordering::Acquire)
    }
}

impl Eq for AtomicCounter {}
