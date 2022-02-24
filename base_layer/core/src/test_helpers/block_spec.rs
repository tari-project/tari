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

use crate::{
    proof_of_work::Difficulty,
    transactions::{tari_amount::MicroTari, transaction_components::Transaction},
};

pub struct BlockSpecs {
    specs: Vec<BlockSpec>,
}

impl BlockSpecs {
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn into_vec(self) -> Vec<BlockSpec> {
        self.specs
    }
}

impl From<Vec<BlockSpec>> for BlockSpecs {
    fn from(specs: Vec<BlockSpec>) -> Self {
        Self { specs }
    }
}

impl<'a, const N: usize> From<&'a [(&'static str, u64, u64); N]> for BlockSpecs {
    fn from(arr: &'a [(&'static str, u64, u64); N]) -> Self {
        BlockSpecs::from(&arr[..])
    }
}

impl<'a> From<&'a [(&'static str, u64, u64)]> for BlockSpecs {
    fn from(arr: &'a [(&'static str, u64, u64)]) -> Self {
        Self {
            specs: arr
                .iter()
                .map(|(name, diff, time)| {
                    BlockSpec::builder()
                        .with_name(name)
                        .with_block_time(*time)
                        .with_difficulty((*diff).into())
                        .finish()
                })
                .collect(),
        }
    }
}

impl IntoIterator for BlockSpecs {
    type IntoIter = std::vec::IntoIter<BlockSpec>;
    type Item = BlockSpec;

    fn into_iter(self) -> Self::IntoIter {
        self.specs.into_iter()
    }
}

#[macro_export]
macro_rules! block_spec {
    (@ { $spec:ident }) => {};

    (@ { $spec: ident } height: $height:expr, $($tail:tt)*) => {
        $spec = $spec.with_height($height);
        $crate::block_spec!(@ { $spec } $($tail)*)
    };
    (@ { $spec: ident } difficulty: $difficulty:expr, $($tail:tt)*) => {
        $spec = $spec.with_difficulty($difficulty.into());
        $crate::block_spec!(@ { $spec } $($tail)*)
    };
    (@ { $spec: ident } reward: $reward:expr, $($tail:tt)*) => {
        $spec = $spec.with_reward($reward.into());
        $crate::block_spec!(@ { spec } $($tail)*)
    };
    (@ { $spec: ident } transactions: $transactions:expr, $($tail:tt)*) => {
        $spec = $spec.with_transactions($transactions.into());
        $crate::block_spec!(@ { spec } $($tail)*)
    };

    (@ { $spec: ident } $k:ident: $v:expr $(,)?) => { $crate::block_spec!(@ { $spec } $k: $v,) };

    ($name:expr, $($tail:tt)+) => {{
        let mut spec = $crate::block_spec!($name);
        $crate::block_spec!(@ { spec } $($tail)+);
        spec.finish()
    }};
    ($name:expr $(,)?) => {
        $crate::test_helpers::BlockSpec::builder().with_name($name).finish()
    };
}

/// Usage:
/// ```ignore
/// block_specs!(["1a->GB"], ["2a->1a"], ["3a->2a", difficulty: 2], ["4a->3a", reward: 50000]);
/// ```
#[macro_export]
macro_rules! block_specs {
    (@ { $specs:ident }) => {};

    (@ { $specs:ident } [$name:expr, $($k:ident: $v:expr),*], $($tail:tt)*) => {
        $specs.push($crate::block_spec!($name, $($k: $v),*));
        block_specs!(@ { $specs } $($tail)*)
    };

    (@ { $specs:ident } [$name:expr $(,)?], $($tail:tt)*) => { block_specs!(@ { $specs } [$name,], $($tail)*) };

    (@ { $specs:ident } [$name:expr $(,)?]$(,)?) => { block_specs!(@ { $specs } [$name,],) };

    (@ { $specs:ident } [$name:expr, $($k:ident: $v:expr),* $(,)?] $(,)?) => { block_specs!(@ { $specs } [$name, $($k: $v),*],) };

    // Entrypoints
    ([$name:expr, $($k:ident: $v:expr),*], $($tail:tt)*) => {
        #[allow(clippy::vec_init_then_push)]
        {
            let mut specs = Vec::new();
            $crate::block_specs!(@ { specs } [$name, $($k: $v),*], $($tail)*);
            $crate::test_helpers::BlockSpecs::from(specs)
        }
    };
    ([$name:expr, $($k:ident: $v:expr),* $(,)?] $(,)*) => {{
        $crate::block_specs!([$name, $($k: $v),*],)
    }};

    ([$name:expr], $($tail:tt)*) => {{ $crate::block_specs!([$name,], $($tail)*) }};

    ([$name:expr]) => {{ $crate::block_specs!([$name,],) }};

    () => { BlockSpecs::from(Vec::new()) };
}

#[derive(Debug, Clone)]
pub struct BlockSpec {
    pub name: &'static str,
    pub prev_block: &'static str,
    pub difficulty: Difficulty,
    pub block_time: u64,
    pub reward_override: Option<MicroTari>,
    pub height_override: Option<u64>,
    pub transactions: Vec<Transaction>,
    pub skip_coinbase: bool,
}

impl BlockSpec {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn builder() -> Self {
        Default::default()
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        let mut split = name.splitn(2, "->");
        let name = split.next().unwrap_or("<noname>");
        self.name = name;
        if let Some(prev_block) = split.next() {
            self.prev_block = prev_block;
        }
        self
    }

    pub fn with_prev_block(mut self, prev_block_name: &'static str) -> Self {
        self.prev_block = prev_block_name;
        self
    }

    pub fn with_height(mut self, height: u64) -> Self {
        self.height_override = Some(height);
        self
    }

    pub fn with_difficulty(mut self, difficulty: Difficulty) -> Self {
        self.difficulty = difficulty;
        self
    }

    pub fn with_block_time(mut self, block_time: u64) -> Self {
        self.block_time = block_time;
        self
    }

    pub fn with_reward(mut self, reward: MicroTari) -> Self {
        self.reward_override = Some(reward);
        self
    }

    pub fn skip_coinbase(mut self) -> Self {
        self.skip_coinbase = true;
        self
    }

    pub fn with_transactions(mut self, transactions: Vec<Transaction>) -> Self {
        self.transactions = transactions;
        self
    }

    pub fn finish(self) -> Self {
        self
    }
}

impl Default for BlockSpec {
    fn default() -> Self {
        Self {
            name: "<unnamed>",
            prev_block: "",
            difficulty: 1.into(),
            block_time: 120,
            height_override: None,
            reward_override: None,
            transactions: vec![],
            skip_coinbase: false,
        }
    }
}
