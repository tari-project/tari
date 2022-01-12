// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{
    blocks::{new_blockheader_template::NewBlockHeaderTemplate, Block},
    proof_of_work::Difficulty,
    transactions::{aggregated_body::AggregateBody, tari_amount::MicroTari},
};

/// The new block template is used constructing a new partial block, allowing a miner to added the coinbase utxo and as
/// a final step the Base node to add the MMR roots to the header.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewBlockTemplate {
    pub header: NewBlockHeaderTemplate,
    pub body: AggregateBody,
    pub target_difficulty: Difficulty,
    pub reward: MicroTari,
    pub total_fees: MicroTari,
}

impl NewBlockTemplate {
    pub fn from_block(block: Block, target_difficulty: Difficulty, reward: MicroTari) -> Self {
        let Block { header, body } = block;
        let total_fees = body.get_total_fee();
        Self {
            header: NewBlockHeaderTemplate::from_header(header),
            body,
            target_difficulty,
            reward,
            total_fees,
        }
    }
}

impl Display for NewBlockTemplate {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        writeln!(f, "----------------- Block template-----------------")?;
        writeln!(f, "--- Header ---")?;
        writeln!(f, "{}", self.header)?;
        writeln!(f, "---  Body ---")?;
        writeln!(f, "{}", self.body)?;
        writeln!(
            f,
            "Target difficulty: {}\nReward: {}\nTotal fees: {}",
            self.target_difficulty, self.reward, self.total_fees
        )?;
        Ok(())
    }
}
