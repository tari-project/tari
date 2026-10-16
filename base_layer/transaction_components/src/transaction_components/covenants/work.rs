// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(test)]
use std::cell::Cell;
use std::cell::OnceCell;

use tari_common_types::types::FixedHash;

use crate::transaction_components::{TransactionOutput, covenants::error::CovenantError};

/// Counts down the work units left for evaluating covenants.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WorkMeter {
    remaining: u64,
}

impl WorkMeter {
    pub fn new(limit: u64) -> Self {
        Self { remaining: limit }
    }

    pub fn unlimited() -> Self {
        Self::new(u64::MAX)
    }

    pub fn charge(&mut self, units: u64) -> Result<(), CovenantError> {
        self.remaining = self
            .remaining
            .checked_sub(units)
            .ok_or(CovenantError::WorkLimitExceeded)?;
        Ok(())
    }
}

/// The hashes of a body's outputs, computed at most once each. One slot per output index.
#[derive(Debug)]
pub(crate) struct OutputHashCache {
    hashes: Vec<OnceCell<FixedHash>>,
    #[cfg(test)]
    computed: Cell<usize>,
}

impl OutputHashCache {
    pub fn new(n: usize) -> Self {
        Self {
            hashes: (0..n).map(|_| OnceCell::new()).collect(),
            #[cfg(test)]
            computed: Cell::new(0),
        }
    }

    /// Returns the hash of `output`, which is the output at `index` in the body.
    pub fn get(&self, index: usize, output: &TransactionOutput) -> FixedHash {
        match self.hashes.get(index) {
            Some(cell) => *cell.get_or_init(|| self.compute(output)),
            None => self.compute(output),
        }
    }

    fn compute(&self, output: &TransactionOutput) -> FixedHash {
        #[cfg(test)]
        self.computed.set(self.computed.get().saturating_add(1));
        output.hash()
    }

    #[cfg(test)]
    pub fn computed(&self) -> usize {
        self.computed.get()
    }

    #[cfg(test)]
    pub fn initialized(&self) -> usize {
        self.hashes.iter().filter(|cell| cell.get().is_some()).count()
    }
}
