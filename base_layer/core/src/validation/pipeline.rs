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

use super::Validation;
use crate::validation::error::ValidationError;
use std::sync::Arc;

type Pipelined<T> = Box<dyn Validation<T>>;

/// A validation pipeline. Multiple validators (structs that implement [Validation]) can be chained together to form a
/// validation pipeline. `ValidationPipeline implements [Validation] itself, and the pipeline is valid _if and only
/// if_ every validator it contains is valid.
///
/// # Example
///
/// ```edition2018
///     use tari_core::validation::{mocks::MockValidator, Validation, ValidationPipeline};
///     use tari_core::blocks::BlockBuilder;
///     use std::sync::Arc;
///
///     let block = Arc::new(BlockBuilder::new().build());
///     let mut pipeline = ValidationPipeline::new();
///     pipeline.push(MockValidator::new(true));
///     assert!(pipeline.validate(block).is_ok());
/// ```

pub struct ValidationPipeline<T> {
    validators: Vec<Pipelined<T>>,
}

impl<T> Default for ValidationPipeline<T> {
    fn default() -> Self {
        ValidationPipeline { validators: Vec::new() }
    }
}

impl<T> ValidationPipeline<T> {
    /// Create a new empty validation pipeline.
    pub fn new() -> Self {
        ValidationPipeline::default()
    }

    /// Add a validator to the pipeline. Validators are executed in the order the are added.
    pub fn push<V: 'static + Validation<T>>(&mut self, validator: V) {
        self.validators.push(Box::new(validator))
    }
}

impl<T> Validation<T> for ValidationPipeline<T> {
    fn validate(&mut self, item: Arc<T>) -> Result<(), ValidationError> {
        for v in &mut self.validators {
            let _ = v.validate(Arc::clone(&item))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        blocks::{Block, BlockBuilder},
        validation::{mocks::MockValidator, Validation, ValidationPipeline},
    };
    use std::sync::Arc;

    #[test]
    fn empty_pipeline() {
        let mut pipeline = ValidationPipeline::<()>::new();
        assert!(pipeline.validate(Arc::new(())).is_ok());
    }

    #[test]
    fn valid_pipeline() {
        let block = Arc::new(BlockBuilder::new().build());
        let mut pipeline = ValidationPipeline::<Block>::new();
        pipeline.push(MockValidator::new(true));
        pipeline.push(MockValidator::new(true));
        pipeline.push(MockValidator::new(true));
        assert!(pipeline.validate(block).is_ok());
    }

    #[test]
    fn invalid_pipeline() {
        let block = Arc::new(BlockBuilder::new().build());
        let mut pipeline = ValidationPipeline::<Block>::new();
        pipeline.push(MockValidator::new(true));
        pipeline.push(MockValidator::new(false));
        pipeline.push(MockValidator::new(true));
        assert!(pipeline.validate(block).is_err());
    }
}
