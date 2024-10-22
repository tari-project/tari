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

/// Holding struct for the "absolute height" filter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsoluteHeightFilter;

impl Filter for AbsoluteHeightFilter {
    // The absolute height filter removes all outputs in the mutable output set if the current block height is less than
    // the absolute height provided in the covenant context.
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let abs_height = context.next_arg()?.require_uint()?;
        let current_height = context.block_height();
        if current_height < abs_height {
            output_set.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        covenant,
        covenants::{filters::test::setup_filter_test, test::create_input},
        transactions::key_manager::create_memory_db_key_manager,
    };

    #[tokio::test]
    async fn it_filters_all_out_if_height_not_reached() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let covenant = covenant!(absolute_height(@uint(100))).unwrap();
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 42, |_| {}, &key_manager).await;

        let mut output_set = OutputSet::new(&outputs);
        AbsoluteHeightFilter.filter(&mut context, &mut output_set).unwrap();

        assert!(output_set.is_empty());
    }

    #[tokio::test]
    async fn it_filters_all_in_if_height_reached() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let covenant = covenant!(absolute_height(@uint(100))).unwrap();
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 100, |_| {}, &key_manager).await;

        let mut output_set = OutputSet::new(&outputs);
        AbsoluteHeightFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 10);
    }

    #[tokio::test]
    async fn it_filters_all_in_if_height_exceeded() {
        let key_manager = create_memory_db_key_manager().unwrap();
        let covenant = covenant!(absolute_height(@uint(42))).unwrap();
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 100, |_| {}, &key_manager).await;

        let mut output_set = OutputSet::new(&outputs);
        AbsoluteHeightFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 10);
    }
}
