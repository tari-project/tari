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
pub struct OrFilter;

impl Filter for OrFilter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let a = context.require_next_filter()?;
        let mut output_set_a = output_set.clone();
        a.filter(context, &mut output_set_a)?;

        let b = context.require_next_filter()?;
        let mut output_set_b = output_set.clone();
        b.filter(context, &mut output_set_b)?;

        output_set.set(output_set_a.union(&output_set_b));
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_script::script;

    use super::*;
    use crate::{
        covenant,
        covenants::{filters::test::setup_filter_test, test::create_input},
        test_helpers::create_test_core_key_manager_with_memory_db,
    };

    #[tokio::test]
    async fn it_filters_outputset_using_union() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let script = script!(CheckHeight(100));
        let covenant = covenant!(or(field_eq(@field::features_maturity, @uint(42),), field_eq(@field::script, @script(script.clone()))));
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(
            &covenant,
            &input,
            0,
            |outputs| {
                outputs[5].features.maturity = 42;
                outputs[5].script = script.clone();
                outputs[7].features.maturity = 42;
                outputs[8].script = script;
            },
            &key_manager,
        )
        .await;
        let mut output_set = OutputSet::new(&outputs);
        OrFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 3);
        assert_eq!(output_set.get_selected_indexes(), vec![5, 7, 8]);
    }
}
