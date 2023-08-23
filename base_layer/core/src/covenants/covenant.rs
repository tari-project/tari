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

use std::{
    io::{self, Write},
    iter::FromIterator,
};

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::{VarIntReader, VarIntWriter};

use super::decoder::CovenantDecodeError;
use crate::{
    common::byte_counter::ByteCounter,
    covenants::{
        context::CovenantContext,
        decoder::CovenantTokenDecoder,
        encoder::CovenantTokenEncoder,
        error::CovenantError,
        filters::Filter,
        output_set::OutputSet,
        token::{CovenantToken, CovenantTokenCollection},
    },
    transactions::transaction_components::{TransactionInput, TransactionOutput},
};

const MAX_COVENANT_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// A covenant allows a UTXO to specify some restrictions on how it is spent in a future transaction.
/// See https://rfc.tari.com/RFC-0250_Covenants.html for details.
pub struct Covenant {
    tokens: Vec<CovenantToken>,
}

impl BorshSerialize for Covenant {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        // writer.write_varint(bytes.len())?;
        writer.write_varint(usize::MAX)?;
        for b in &bytes {
            b.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for Covenant {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len > MAX_COVENANT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than max covenant bytes".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let covenant = Self::from_bytes(&mut data.as_slice())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        Ok(covenant)
    }
}

impl Covenant {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Produces a new `Covenant` instance, out of a byte buffer. It errors
    /// if the byte buffer length is higher than `MAX_COVENANT_BYTES`.
    pub fn from_bytes(bytes: &mut &[u8]) -> Result<Self, CovenantDecodeError> {
        if bytes.is_empty() {
            return Ok(Self::new());
        }
        if bytes.len() > MAX_COVENANT_BYTES {
            return Err(CovenantDecodeError::ExceededMaxBytes);
        }
        CovenantTokenDecoder::new(bytes).collect()
    }

    /// Given a `Covenant` instance, it writes its bytes content to a
    /// new byte buffer.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.get_byte_length());
        self.write_to(&mut buf).unwrap();
        buf
    }

    /// Writes a `Covenant` instance byte to a writer.
    pub(super) fn write_to<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        CovenantTokenEncoder::new(self.tokens.as_slice()).write_to(writer)
    }

    /// Gets the byte lenght of the underlying byte buffer
    pub(super) fn get_byte_length(&self) -> usize {
        let mut counter = ByteCounter::new();
        self.write_to(&mut counter).unwrap();
        counter.get()
    }

    /// It executes the covenant on the transaction input being spent, it filters the transaction outputs which should
    /// generate at least one match. An empty covenant is an identity and matches all outputs.
    pub fn execute(
        &self,
        block_height: u64,
        input: &TransactionInput,
        outputs: &[TransactionOutput],
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

    /// Adds a new `CovenantToken` to the current `tokens` vector field.
    pub fn push_token(&mut self, token: CovenantToken) {
        self.tokens.push(token);
    }

    #[cfg(test)]
    /// Outputs a slice of the instance existing `CovenantToken`'s.
    pub(super) fn tokens(&self) -> &[CovenantToken] {
        &self.tokens
    }

    /// Outputs the length of `tokens` field.
    pub fn num_tokens(&self) -> usize {
        self.tokens.len()
    }

    /// Checks if the `tokens` field is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

impl FromIterator<CovenantToken> for Covenant {
    /// Creates a new `CovenantToken` instance from an iterator with `Item = CovenantToken`.
    fn from_iter<T: IntoIterator<Item = CovenantToken>>(iter: T) -> Self {
        Self {
            tokens: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod test {
    use borsh::{BorshDeserialize, BorshSerialize};

    use crate::{
        covenant,
        covenants::{
            test::{create_input, create_outputs},
            Covenant,
        },
        transactions::test_helpers::{create_test_core_key_manager_with_memory_db, UtxoTestParams},
    };

    #[tokio::test]
    async fn it_succeeds_when_empty() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let outputs = create_outputs(10, UtxoTestParams::default(), &key_manager).await;
        let input = create_input(&key_manager).await;
        let covenant = covenant!();
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 10);
    }

    #[tokio::test]
    async fn it_executes_the_covenant() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let mut outputs = create_outputs(10, UtxoTestParams::default(), &key_manager).await;
        outputs[4].features.maturity = 42;
        outputs[5].features.maturity = 42;
        outputs[7].features.maturity = 42;
        let mut input = create_input(&key_manager).await;
        input.set_maturity(42).unwrap();
        let covenant = covenant!(fields_preserved(@fields(
            @field::features_output_type,
            @field::features_maturity))
        );
        let num_matching_outputs = covenant.execute(0, &input, &outputs).unwrap();
        assert_eq!(num_matching_outputs, 3);
    }

    #[tokio::test]
    async fn test_borsh_de_serialization() {
        let key_manager = create_test_core_key_manager_with_memory_db();
        let mut outputs = create_outputs(10, UtxoTestParams::default(), &key_manager).await;
        outputs[4].features.maturity = 42;
        outputs[5].features.maturity = 42;
        outputs[7].features.maturity = 42;
        let mut input = create_input(&key_manager).await;
        input.set_maturity(42).unwrap();
        let covenant = covenant!(fields_preserved(@fields(
            @field::features_output_type,
            @field::features_maturity))
        );
        let mut buf = Vec::new();
        covenant.serialize(&mut buf).unwrap();
        buf.extend_from_slice(&[1, 2, 3]);
        let buf = &mut buf.as_slice();
        assert_eq!(covenant, Covenant::deserialize(buf).unwrap());
        assert_eq!(buf, &[1, 2, 3]);
    }

    #[tokio::test]
    async fn test_borsh_de_serialization_too_large() {
        // We dont care about the actual convent here, just that its not too large on the varint size
        // We lie about the size to try and get a mem panic, and say this covenant is u64::max large.
        let buf = vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1, 49, 8, 2, 5, 6];
        let buf = &mut buf.as_slice();
        assert!(Covenant::deserialize(buf).is_err());
    }
}
