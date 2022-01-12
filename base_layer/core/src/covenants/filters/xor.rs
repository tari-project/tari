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
pub struct XorFilter;

impl Filter for XorFilter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let a = context.require_next_filter()?;
        let mut output_set_a = output_set.clone();
        a.filter(context, &mut output_set_a)?;

        let b = context.require_next_filter()?;
        let mut output_set_b = output_set.clone();
        b.filter(context, &mut output_set_b)?;

        output_set.set(output_set_a.symmetric_difference(output_set_b));
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::{
        covenant,
        covenants::{filters::test::setup_filter_test, test::create_input},
    };

    #[test]
    fn it_filters_outputset_using_symmetric_difference() {
        let bytes = vec![0xab, 0xcd, 0xef];
        let covenant = covenant!(or(field_eq(@field::features_maturity, @uint(42),), field_eq(@field::features_unique_id, @bytes(bytes.clone()))));
        let input = create_input();
        let (mut context, outputs) = setup_filter_test(&covenant, &input, 0, |outputs| {
            outputs[5].features.maturity = 42;
            outputs[5].features.unique_id = Some(bytes.clone());
            outputs[7].features.maturity = 42;
            outputs[8].features.unique_id = Some(bytes.clone());
        });
        let mut output_set = OutputSet::new(&outputs);
        XorFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 2);
        assert_eq!(output_set.get_selected_indexes(), vec![7, 8]);
    }
}
