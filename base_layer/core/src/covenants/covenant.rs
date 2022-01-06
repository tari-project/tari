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

use std::{io, iter::FromIterator};

use crate::{
    common::byte_counter::ByteCounter,
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    covenants::{
        context::CovenantContext,
        decoder::{CovenantDecodeError, CovenantTokenDecoder},
        encoder::CovenantTokenEncoder,
        error::CovenantError,
        filters::Filter,
        output_set::OutputSet,
        token::{CovenantToken, CovenantTokenCollection},
    },
    transactions::transaction::{TransactionInput, TransactionOutput},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Covenant {
    tokens: Vec<CovenantToken>,
}

impl Covenant {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    pub fn from_bytes(mut bytes: &[u8]) -> Result<Self, CovenantDecodeError> {
        if bytes.is_empty() {
            return Ok(Self::new());
        }
        CovenantTokenDecoder::new(&mut bytes).collect()
    }

    pub(super) fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        CovenantTokenEncoder::new(self.tokens.as_slice()).write_to(writer)
    }

    pub fn execute<'a>(
        &self,
        block_height: u64,
        input: &TransactionInput,
        outputs: &'a [TransactionOutput],
    ) -> Result<usize, CovenantError> {
        if self.tokens.is_empty() {
            // Empty covenants always pass
            return Ok(outputs.len());
        }

        let tokens = CovenantTokenCollection::from_iter(self.tokens.clone());
        let mut cx = CovenantContext::new(tokens, input, block_height);
        let root = cx.require_next_filter()?;
        let mut output_set = OutputSet::new(outputs);
        root.filter(&mut cx, &mut output_set)?;
        if cx.has_more_tokens() {
            return Err(CovenantError::RemainingTokens);
        }
        if output_set.is_empty() {
            return Err(CovenantError::NoMatchingOutputs);
        }

        Ok(output_set.len())
    }

    pub fn push_token(&mut self, token: CovenantToken) {
        self.tokens.push(token);
    }

    #[cfg(test)]
    pub(super) fn tokens(&self) -> &[CovenantToken] {
        &self.tokens
    }
}

impl ConsensusEncoding for Covenant {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        self.write_to(writer)
    }
}

impl ConsensusEncodingSized for Covenant {
    fn consensus_encode_exact_size(&self) -> usize {
        let mut byte_counter = ByteCounter::new();
        self.write_to(&mut byte_counter).expect("unreachable panic");
        byte_counter.get()
    }
}

impl ConsensusDecoding for Covenant {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        CovenantTokenDecoder::new(reader)
            .collect::<Result<_, CovenantDecodeError>>()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))
    }
}

impl FromIterator<CovenantToken> for Covenant {
    fn from_iter<T: IntoIterator<Item = CovenantToken>>(iter: T) -> Self {
        Self {
            tokens: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        covenant,
        covenants::test::{create_input, create_outputs},
    };

    #[test]
    fn it_succeeds_when_empty() {
        let outputs = create_outputs(10, Default::default());
        let input = create_input();
        let covenant = covenant!();
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 10);
    }

    #[test]
    fn it_executes_the_covenant() {
        let mut outputs = create_outputs(10, Default::default());
        outputs[4].features.maturity = 42;
        outputs[5].features.maturity = 42;
        outputs[7].features.maturity = 42;
        let mut input = create_input();
        input.features.maturity = 42;
        let covenant = covenant!(fields_preserved(@fields(@field::features)));
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 3);
    }
}
