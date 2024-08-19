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

/// Holding struct for the "output hash equal" filter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputHashEqFilter;

impl Filter for OutputHashEqFilter {
    // The output hash equal filter searches for the hashed output field equal to the specified hash value
    // based on the covenant context; either returning the output or nothing
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let hash = context.next_arg()?.require_hash()?;
        // An output's hash is unique so the output set is either 1 or 0 outputs will match
        output_set.find_inplace(|output| *output.hash().as_slice() == hash);
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::{
        covenant,
        covenants::{
            filters::test::setup_filter_test,
            test::{create_input, create_outputs},
        },
        transactions::key_manager::create_memory_db_key_manager,
    };

    #[tokio::test]
    async fn it_filters_output_with_specific_hash() -> Result<(), Box<dyn std::error::Error>> {
        let key_manager = create_memory_db_key_manager().unwrap();
        let output = create_outputs(1, Default::default(), &key_manager).await.remove(0);
        let output_hash = output.hash();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(output_hash.as_slice());
        let covenant = covenant!(output_hash_eq(@hash(hash.into()))).unwrap();
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(
            &covenant,
            &input,
            0,
            move |outputs| {
                outputs.insert(5, output);
            },
            &key_manager,
        )
        .await;
        let mut output_set = OutputSet::new(&outputs);
        OutputHashEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 1);
        assert_eq!(output_set.get_selected_indexes(), vec![5]);
        Ok(())
    }
}
