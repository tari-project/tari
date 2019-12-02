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
use crate::validation::{error::ValidationError, ValidationPipeline};
use std::sync::Arc;

pub struct MockValidator {
    result: bool,
}

impl MockValidator {
    pub fn new(is_valid: bool) -> MockValidator {
        MockValidator { result: is_valid }
    }
}

impl<T> Validation<T> for MockValidator {
    fn validate(&mut self, _item: Arc<T>) -> Result<(), ValidationError> {
        match self.result {
            true => Ok(()),
            false => Err(ValidationError::CustomError(
                "This mock validator always returns an error".into(),
            )),
        }
    }
}

/// Creates a [ValidationPipeline] that always validates
pub fn new_valid_pipeline<T>() -> ValidationPipeline<T> {
    ValidationPipeline::default()
}

/// Creates a [ValidationPipeline] that always returns an error
pub fn new_invalid_pipeline<T>() -> ValidationPipeline<T> {
    let mut pipeline = ValidationPipeline::default();
    pipeline.push(MockValidator::new(false));
    pipeline
}

#[cfg(test)]
mod test {
    use crate::validation::{
        mocks::{new_invalid_pipeline, new_valid_pipeline, MockValidator},
        Validation,
    };
    use std::sync::Arc;

    #[test]
    fn mock_is_valid() {
        let mut validator = MockValidator::new(true);
        assert!(validator.validate(Arc::new(())).is_ok());
    }

    #[test]
    fn mock_is_invalid() {
        let mut validator = MockValidator::new(false);
        assert!(validator.validate(Arc::new(())).is_err());
    }

    #[test]
    fn valid_pipeline() {
        let mut pipeline = new_valid_pipeline();
        assert!(pipeline.validate(Arc::new(())).is_ok());
    }

    #[test]
    fn invalid_pipeline() {
        let mut pipeline = new_invalid_pipeline();
        assert!(pipeline.validate(Arc::new(())).is_err());
    }
}
