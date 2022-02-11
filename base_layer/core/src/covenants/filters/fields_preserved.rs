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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldsPreservedFilter;

impl Filter for FieldsPreservedFilter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let fields = context.next_arg()?.require_outputfields()?;
        let input = context.input();
        output_set.retain(|output| Ok(fields.iter().all(|field| field.is_eq_input(input, output))))?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::{
        covenant,
        covenants::{filters::test::setup_filter_test, test::create_input},
        transactions::transaction_components::OutputFlags,
    };

    #[test]
    fn it_filters_outputs_that_match_input_fields() {
        let bytes = vec![0xab, 0xcd, 0xef];
        let covenant = covenant!(fields_preserved(@fields(@field::features_maturity, @field::features_unique_id, @field::features_flags)));
        let mut input = create_input();
        input.set_maturity(42).unwrap();
        input.features_mut().unwrap().unique_id = Some(bytes.clone());
        input.features_mut().unwrap().flags = OutputFlags::SIDECHAIN_CHECKPOINT;
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 0, |outputs| {
            outputs[5].features.maturity = 42;
            outputs[5].features.unique_id = Some(bytes.clone());
            outputs[5].features.flags = OutputFlags::SIDECHAIN_CHECKPOINT;
            outputs[7].features.maturity = 42;
            outputs[7].features.flags = OutputFlags::SIDECHAIN_CHECKPOINT;
            outputs[7].features.unique_id = Some(vec![0x01, 0x02]);
            outputs[8].features.maturity = 42;
            outputs[8].features.unique_id = Some(bytes.clone());
            outputs[8].features.flags = OutputFlags::SIDECHAIN_CHECKPOINT | OutputFlags::COINBASE_OUTPUT;
        });
        let mut output_set = OutputSet::new(&outputs);

        FieldsPreservedFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 1);
        assert_eq!(output_set.get_selected_indexes(), vec![5]);
    }
}
