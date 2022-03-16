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

use crate::covenants::{
    arguments::CovenantArg,
    context::CovenantContext,
    error::CovenantError,
    filters::Filter,
    output_set::OutputSet,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEqFilter;

impl Filter for FieldEqFilter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let field = context.next_arg()?.require_outputfield()?;
        let arg = context.next_arg()?;
        output_set.retain(|output| {
            use CovenantArg::*;
            match &arg {
                Hash(hash) => field.is_eq(output, hash),
                PublicKey(pk) => field.is_eq(output, pk),
                Commitment(commitment) => field.is_eq(output, commitment),
                TariScript(script) => field.is_eq(output, script),
                Covenant(covenant) => field.is_eq(output, covenant),
                Uint(int) => {
                    let val = field
                        .get_field_value_ref::<u64>(output)
                        .copied()
                        .or_else(|| field.get_field_value_ref::<u32>(output).map(|v| *v as u64));

                    match val {
                        Some(val) => Ok(val == *int),
                        None => Err(CovenantError::InvalidArgument {
                            filter: "fields_eq",
                            details: "Uint argument cannot be compared to non-numeric field".to_string(),
                        }),
                    }
                },
                Bytes(bytes) => field.is_eq(output, bytes),
                OutputField(_) | OutputFields(_) => Err(CovenantError::InvalidArgument {
                    filter: "field_eq",
                    details: "Invalid argument: fields are not a valid argument for field_eq".to_string(),
                }),
            }
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::{Commitment, PublicKey};
    use tari_script::script;
    use tari_test_utils::unpack_enum;
    use tari_utilities::hex::Hex;

    use super::*;
    use crate::{
        covenant,
        covenants::test::{create_context, create_input, create_outputs},
    };

    #[test]
    fn it_filters_uint() {
        let covenant = covenant!(field_eq(@field::features_maturity, @uint(42)));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let mut outputs = create_outputs(10, Default::default());
        outputs[5].features.maturity = 42;
        let mut output_set = OutputSet::new(&outputs);
        FieldEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 1);
        assert_eq!(output_set.get(5).unwrap().features.maturity, 42);
    }

    #[test]
    fn it_filters_sender_offset_public_key() {
        let pk = PublicKey::from_hex("5615a327e1d19da34e5aa8bbd2ecc97addf29b158844b885bfc4efa0dab17052").unwrap();
        let covenant = covenant!(field_eq(
            @field::features_parent_public_key,
            @public_key(pk.clone())
        ));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let mut outputs = create_outputs(10, Default::default());
        outputs[5].features.parent_public_key = Some(pk.clone());
        let mut output_set = OutputSet::new(&outputs);
        FieldEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 1);
        assert_eq!(
            *output_set.get(5).unwrap().features.parent_public_key.as_ref().unwrap(),
            pk
        );
    }

    #[test]
    fn it_filters_commitment() {
        let commitment =
            Commitment::from_hex("7ca31ba517d8b563609ed6707fedde5a2be64ac1d67b254cb5348bc2f680557f").unwrap();
        let covenant = covenant!(field_eq(
            @field::commitment,
            @commitment(commitment.clone())
        ));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let mut outputs = create_outputs(10, Default::default());
        outputs[5].commitment = commitment.clone();
        outputs[7].commitment = commitment;
        let mut output_set = OutputSet::new(&outputs);
        FieldEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 2);
        assert_eq!(output_set.get_selected_indexes(), vec![5, 7]);
    }

    #[test]
    fn it_filters_tari_script() {
        let script = script!(CheckHeight(100));
        let covenant = covenant!(field_eq(
            @field::script,
            @script(script.clone())
        ));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let mut outputs = create_outputs(10, Default::default());
        outputs[5].script = script.clone();
        outputs[7].script = script;
        let mut output_set = OutputSet::new(&outputs);
        FieldEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 2);
        assert_eq!(output_set.get_selected_indexes(), vec![5, 7]);
    }

    // #[test]
    // fn it_filters_covenant() {
    //     // TODO: Covenant field is not in output yet
    //     let covenant = covenant!(identity());
    //     let covenant = covenant!(field_eq(
    //         @field::covenant,
    //         @covenant(covenant.clone())
    //     ));
    //     let input = create_input();
    //     let mut context = create_context(&covenant, &input, 0);
    //     // Remove `field_eq`
    //     context.next_filter().unwrap();
    //     let mut outputs = create_outputs(10, Default::default());
    //     outputs[5].covenant = covenant.clone();
    //     outputs[7].covenant = covenant.clone();
    //     let mut output_set = OutputSet::new(&outputs);
    //     FieldEqFilter.filter(&mut context, &mut output_set).unwrap();
    //
    //     assert_eq!(output_set.len(), 2);
    //     assert_eq!(output_set.get_selected_indexes(), vec![5, 7]);
    // }

    #[test]
    fn it_errors_for_unsupported_features_field() {
        let covenant = covenant!(field_eq(
            @field::features,
            @bytes(vec![])
        ));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let outputs = create_outputs(10, Default::default());
        let mut output_set = OutputSet::new(&outputs);
        let err = FieldEqFilter.filter(&mut context, &mut output_set).unwrap_err();
        unpack_enum!(CovenantError::UnsupportedArgument { arg, .. } = err);
        assert_eq!(arg, "features");
    }

    #[test]
    fn it_errors_if_field_has_an_incorrect_type() {
        let covenant = covenant!(field_eq(@field::features, @uint(42)));
        let input = create_input();
        let mut context = create_context(&covenant, &input, 0);
        // Remove `field_eq`
        context.next_filter().unwrap();
        let outputs = create_outputs(10, Default::default());
        let mut output_set = OutputSet::new(&outputs);
        let err = FieldEqFilter.filter(&mut context, &mut output_set).unwrap_err();
        unpack_enum!(CovenantError::InvalidArgument { .. } = err);
    }
}
