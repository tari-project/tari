//  Copyright 2020, The Tari Project
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

use crate::validation::{StatelessValidation, ValidationError};

pub struct AndThenValidator<T, U> {
    first: T,
    second: U,
}

impl<T, U> AndThenValidator<T, U> {
    pub fn new(first: T, second: U) -> Self {
        Self { first, second }
    }
}

impl<T, U, I> StatelessValidation<I> for AndThenValidator<T, U>
where
    T: StatelessValidation<I>,
    U: StatelessValidation<I>,
{
    fn validate(&self, item: &I) -> Result<(), ValidationError> {
        self.first.validate(item)?;
        self.second.validate(item)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::validation::mocks::MockValidator;

    #[test]
    fn validation_succeeds() {
        let validator = AndThenValidator::new(MockValidator::new(true), MockValidator::new(true));
        validator.validate(&1).unwrap();
    }

    #[test]
    fn validation_fails() {
        let validator = AndThenValidator::new(MockValidator::new(false), MockValidator::new(true));
        validator.validate(&1).unwrap_err();
        let validator = AndThenValidator::new(MockValidator::new(true), MockValidator::new(false));
        validator.validate(&1).unwrap_err();
        let validator = AndThenValidator::new(MockValidator::new(false), MockValidator::new(false));
        validator.validate(&1).unwrap_err();
    }
}
