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

use digest::Digest;

use crate::covenants::{context::CovenantContext, error::CovenantError, filters::Filter, output_set::OutputSet};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldsHashedEqFilter;

impl Filter for FieldsHashedEqFilter {
    fn filter(&self, context: &mut CovenantContext<'_>, output_set: &mut OutputSet<'_>) -> Result<(), CovenantError> {
        let fields = context.next_arg()?.require_outputfields()?;
        let hash = context.next_arg()?.require_hash()?;
        output_set.retain(|output| {
            let challenge = fields.construct_challenge_from(output).finalize();
            Ok(challenge[..] == *hash)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use borsh::BorshSerialize;
    use tari_common_types::types::Challenge;
    use tari_crypto::hashing::DomainSeparation;

    use super::*;
    use crate::{
        covenant,
        covenants::{
            filters::test::setup_filter_test,
            test::{create_input, make_sample_sidechain_feature},
            BaseLayerCovenantsDomain,
            COVENANTS_FIELD_HASHER_LABEL,
        },
        transactions::{
            test_helpers::create_test_core_key_manager_with_memory_db,
            transaction_components::OutputFeatures,
        },
    };

    #[tokio::test]
    async fn it_filters_outputs_with_fields_that_hash_to_given_hash() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let features = OutputFeatures {
            maturity: 42,
            sidechain_feature: Some(make_sample_sidechain_feature()),
            ..Default::default()
        };
        let mut hasher = Challenge::new();
        BaseLayerCovenantsDomain::add_domain_separation_tag(&mut hasher, COVENANTS_FIELD_HASHER_LABEL);
        let hash = hasher.chain(features.try_to_vec().unwrap()).finalize();
        let covenant = covenant!(fields_hashed_eq(@fields(@field::features), @hash(hash.into())));
        let input = create_input(&key_manager).await;
        let (mut context, outputs) = setup_filter_test(
            &covenant,
            &input,
            0,
            |outputs| {
                outputs[5].features = features.clone();
                outputs[7].features = features;
            },
            &key_manager,
        )
        .await;
        let mut output_set = OutputSet::new(&outputs);
        FieldsHashedEqFilter.filter(&mut context, &mut output_set).unwrap();

        assert_eq!(output_set.len(), 2);
        assert_eq!(output_set.get_selected_indexes(), vec![5, 7]);
    }
}
