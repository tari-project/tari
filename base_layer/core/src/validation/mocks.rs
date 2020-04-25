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

use super::{StatelessValidation, Validation};
use crate::{chain_storage::BlockchainBackend, validation::error::ValidationError};

#[derive(Clone)]
pub struct MockValidator {
    result: bool,
}

impl MockValidator {
    pub fn new(is_valid: bool) -> MockValidator {
        MockValidator { result: is_valid }
    }
}

impl<T, B: BlockchainBackend> Validation<T, B> for MockValidator {
    fn validate(&self, _item: &T, _db: &B) -> Result<(), ValidationError> {
        if self.result {
            Ok(())
        } else {
            Err(ValidationError::CustomError(
                "This mock validator always returns an error".into(),
            ))
        }
    }
}

impl<T> StatelessValidation<T> for MockValidator {
    fn validate(&self, _item: &T) -> Result<(), ValidationError> {
        if self.result {
            Ok(())
        } else {
            Err(ValidationError::CustomError(
                "This mock validator always returns an error".into(),
            ))
        }
    }
}

#[cfg(test)]
mod test {
    use crate::validation::{mocks::MockValidator, StatelessValidation};

    #[test]
    fn mock_is_valid() {
        let validator = MockValidator::new(true);
        assert!(<MockValidator as StatelessValidation<_>>::validate(&validator, &()).is_ok());
    }

    #[test]
    fn mock_is_invalid() {
        let validator = MockValidator::new(false);
        assert!(<MockValidator as StatelessValidation<_>>::validate(&validator, &()).is_err());
    }
}
