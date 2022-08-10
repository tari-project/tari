// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::{Display, Formatter};

use tari_common_types::types::{FixedHash, PublicKey};
use tari_crypto::hash::blake2::Blake256;
use tari_dan_common_types::TemplateId;
use tari_utilities::hex::Hex;

use super::hashing::{dan_layer_engine_instructions, INSTRUCTION_LABEL};

#[derive(Clone, Debug)]
pub struct Instruction {
    template_id: TemplateId,
    method: String,
    args: Vec<u8>,
    sender: PublicKey,
    // from: TokenId,
    // signature: ComSig,
    hash: FixedHash,
}

impl PartialEq for Instruction {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl Instruction {
    pub fn new(
        template_id: TemplateId,
        method: String,
        args: Vec<u8>,
        sender: PublicKey,
        // from: TokenId,
        // _signature: ComSig,
    ) -> Self {
        let mut s = Self {
            template_id,
            method,
            args,
            sender,
            // from,
            // TODO: this is obviously wrong
            // signature: ComSig::default(),
            hash: FixedHash::zero(),
        };
        s.hash = s.calculate_hash();
        s
    }

    pub fn template_id(&self) -> TemplateId {
        self.template_id
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn args(&self) -> &[u8] {
        &self.args
    }

    pub fn sender(&self) -> PublicKey {
        self.sender.clone()
    }

    // // TODO: rename to avoid use of from
    // pub fn from_owner(&self) -> &TokenId {
    //     &self.from
    // }

    // pub fn _signature(&self) -> &ComSig {
    //     &self.signature
    // }

    pub fn hash(&self) -> &FixedHash {
        &self.hash
    }

    pub fn calculate_hash(&self) -> FixedHash {
        // Blake256 has 32-byte output
        let b = dan_layer_engine_instructions::<Blake256>(INSTRUCTION_LABEL)
            .chain(self.method.as_bytes())
            .chain(&self.args)
            .finalize();

        let mut out = [0u8; 32];
        out.copy_from_slice(b.as_ref());

        out.into()
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Method: {}, Hash: {}, Args: {} bytes, Template: {}",
            self.method,
            self.hash.to_hex(),
            self.args.len(),
            self.template_id
        )
    }
}
