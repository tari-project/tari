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

use crate::validation::{and_then::AndThenValidator, error::ValidationError};

pub type StatefulValidator<T, B> = Box<dyn StatefulValidation<T, B>>;
pub type Validator<T> = Box<dyn Validation<T>>;

/// The "stateful" version of the `Validation` trait. This trait allows extra context to be passed through to the
/// validate function. Typically this will be an implementation of the `BlockchainBackend` trait.
pub trait StatefulValidation<T, B>: Send + Sync {
    /// General validation code that can run independent of external state
    fn validate(&self, item: &T, db: &B) -> Result<(), ValidationError>;
}

/// The core validation trait.
/// Multiple validators can be chained together by using the `and_then` combinator.
pub trait Validation<T>: Send + Sync {
    /// General validation code that can run independent of external state
    fn validate(&self, item: &T) -> Result<(), ValidationError>;
}

pub trait ValidationExt<T>: Validation<T> {
    /// Creates a new validator that performs this validation followed by another. If the first validation fails, the
    /// second one is not run.
    fn and_then<V: Validation<T>>(self, other: V) -> AndThenValidator<Self, V>
    where Self: Sized {
        AndThenValidator::new(self, other)
    }
}
impl<T, U> ValidationExt<T> for U where U: Validation<T> {}
