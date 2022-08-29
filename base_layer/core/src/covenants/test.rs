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

use std::{convert::TryInto, iter};

use crate::{
    covenants::{context::CovenantContext, Covenant},
    transactions::{
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::{
            BuildInfo,
            CodeTemplateRegistration,
            SideChainFeatures,
            TemplateType,
            TransactionInput,
            TransactionOutput,
        },
    },
};

pub fn create_outputs(n: usize, utxo_params: UtxoTestParams) -> Vec<TransactionOutput> {
    iter::repeat_with(|| {
        let params = TestParams::new();
        let output = params.create_unblinded_output(utxo_params.clone());
        output.as_transaction_output(&Default::default()).unwrap()
    })
    .take(n)
    .collect()
}

pub fn create_input() -> TransactionInput {
    let params = TestParams::new();
    let output = params.create_unblinded_output(Default::default());
    output.as_transaction_input(&Default::default()).unwrap()
}

pub fn create_context<'a>(covenant: &Covenant, input: &'a TransactionInput, block_height: u64) -> CovenantContext<'a> {
    let tokens = covenant.tokens().to_vec();
    CovenantContext::new(tokens.into(), input, block_height)
}

pub fn make_sample_sidechain_features() -> SideChainFeatures {
    let template_reg = CodeTemplateRegistration {
        author_public_key: Default::default(),
        author_signature: Default::default(),
        template_name: "test".to_string().try_into().unwrap(),
        template_version: 0,
        template_type: TemplateType::Wasm { abi_version: 0 },
        build_info: BuildInfo {
            repo_url: "https://github.com/tari-project/tari.git".try_into().unwrap(),
            commit_hash: Default::default(),
        },
        binary_sha: Default::default(),
        binary_url: "https://github.com/tari-project/tari.git".try_into().unwrap(),
    };
    SideChainFeatures::TemplateRegistration(template_reg)
}
