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

use crate::covenants::{context::CovenantContext, error::CovenantError, filters::Filter, output_set::OutputSet};

/// Holding struct for the "identity" filter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFilter;

impl Filter for IdentityFilter {
    /// The identity filter does not filter the output set.
    fn filter(&self, _: &mut CovenantContext<'_>, _: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        covenant,
        covenants::{filters::test::setup_filter_test, test::create_input},
        transactions::test_helpers::create_test_core_key_manager_with_memory_db,
    };

    #[tokio::test]
    async fn it_returns_the_outputset_unchanged() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let covenant = covenant!(identity());
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 0, |_| {}, &key_manager).await;
        let mut output_set = OutputSet::new(&outputs);
        let previous_len = output_set.len();
        IdentityFilter.filter(&mut context, &mut output_set).unwrap();
        assert_eq!(output_set.len(), previous_len);
    }
}
