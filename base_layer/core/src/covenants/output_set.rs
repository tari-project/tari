//  Copyright 2021, The Tari Project
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

use std::{
    cmp::Ordering,
    collections::BTreeSet,
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

use crate::{covenants::error::CovenantError, transactions::transaction_components::TransactionOutput};

#[derive(Debug, Clone)]
pub struct OutputSet<'a>(BTreeSet<Indexed<&'a TransactionOutput>>);

impl<'a> OutputSet<'a> {
    pub fn new(outputs: &'a [TransactionOutput]) -> Self {
        // This sets the internal index for each output
        // Note there is no publicly accessible way to modify the indexes
        outputs.iter().enumerate().collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn set(&mut self, new_set: Self) {
        *self = new_set;
    }

    pub fn retain<F>(&mut self, mut f: F) -> Result<(), CovenantError>
    where F: FnMut(&'a TransactionOutput) -> Result<bool, CovenantError> {
        let mut err = None;
        self.0.retain(|output| match f(**output) {
            Ok(b) => b,
            Err(e) => {
                // Theres no way to stop retain early, so keep the error for when this completes
                err = Some(e);
                false
            },
        });
        match err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        self.0.union(&other.0).copied().collect()
    }

    pub fn difference(&self, other: &Self) -> Self {
        self.0.difference(&other.0).copied().collect()
    }

    pub fn symmetric_difference(&self, other: Self) -> Self {
        self.0.symmetric_difference(&other.0).copied().collect()
    }

    pub fn find_inplace<F>(&mut self, mut pred: F)
    where F: FnMut(&TransactionOutput) -> bool {
        match self.0.iter().find(|indexed| pred(&**indexed)) {
            Some(output) => {
                let output = *output;
                self.clear();
                self.0.insert(output);
            },
            None => {
                self.clear();
            },
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    #[cfg(test)]
    pub(super) fn get(&self, index: usize) -> Option<&TransactionOutput> {
        self.0
            .iter()
            .find(|output| output.index == index)
            .map(|output| **output)
    }

    #[cfg(test)]
    pub(super) fn get_selected_indexes(&self) -> Vec<usize> {
        self.0.iter().map(|idx| idx.index).collect()
    }
}

impl<'a> FromIterator<(usize, &'a TransactionOutput)> for OutputSet<'a> {
    fn from_iter<T: IntoIterator<Item = (usize, &'a TransactionOutput)>>(iter: T) -> Self {
        iter.into_iter().map(|(i, output)| Indexed::new(i, output)).collect()
    }
}

impl<'a> FromIterator<Indexed<&'a TransactionOutput>> for OutputSet<'a> {
    fn from_iter<T: IntoIterator<Item = Indexed<&'a TransactionOutput>>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// A simple wrapper struct that implements PartialEq and PartialOrd using a numeric index
#[derive(Debug, Clone, Copy)]
struct Indexed<T> {
    index: usize,
    value: T,
}

impl<T> Indexed<T> {
    pub fn new(index: usize, value: T) -> Self {
        Self { index, value }
    }
}

impl<T> PartialEq for Indexed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for Indexed<T> {}

impl<T> PartialOrd for Indexed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.index.partial_cmp(&other.index)
    }
}

impl<T> Ord for Indexed<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.index.cmp(&other.index)
    }
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Indexed<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
