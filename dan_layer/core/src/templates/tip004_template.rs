//  Copyright 2021. The Tari Project
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
use log::*;
use prost::Message;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_crypto::{common::Blake256, tari_utilities::hex::Hex};
use tari_dan_common_types::proto::tips::tip004;

use crate::{
    models::InstructionSet,
    storage::state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::dan_layer::core::templates::tip004_template";

pub fn initial_instructions(_: &TemplateParameter) -> InstructionSet {
    InstructionSet::empty()
}

pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
    method: &str,
    args: &[u8],
    state_db: &TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "balanceof" => balance_of(args, state_db),
        "tokenofownerbyindex" => token_of_owner_by_index(args, state_db),
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
    method: &str,
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<(), DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "mint" => mint(args, state_db),
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

fn mint<TUnitOfWork: StateDbUnitOfWork>(args: &[u8], state_db: &mut TUnitOfWork) -> Result<(), DigitalAssetError> {
    let request = tip004::MintRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip004::MintRequest".to_string(),
    })?;

    let token: String = request.token.clone();
    let owner: Vec<u8> = request.owner;

    let hash = hash_of(&token);

    match state_db.get_u64("id_of", &hash)? {
        Some(_id) => {
            // unimplemented!("Token has already been minted");
            error!(target: LOG_TARGET, "Token has already been minted");
        },
        None => {
            let total_supply = state_db.get_u64("info", "total_supply".as_bytes())?.unwrap_or_default();
            state_db.set_u64("id_of", &hash, total_supply)?;
            state_db.set_u64("info", "total_supply".as_bytes(), total_supply + 1)?;

            state_db.set_value("owners".to_string(), total_supply.to_le_bytes().to_vec(), owner)?;
            // TODO: this might be too specific
            state_db.set_value(
                "tokens".to_string(),
                total_supply.to_le_bytes().to_vec(),
                Vec::from(token.as_bytes()),
            )?;
        },
    }

    Ok(())
}

fn hash_of(s: &str) -> Vec<u8> {
    Blake256::new().chain(s).finalize().to_vec()
}

fn balance_of<TUnitOfWork: StateDbUnitOfWorkReader>(
    args: &[u8],
    state_db: &TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    // TODO: move this to the invoke_read_method method
    let request = tip004::BalanceOfRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip004::BalanceOfRequest".to_string(),
    })?;

    let owner = request.owner;
    let owner_records = state_db.find_keys_by_value("owners", &owner)?;
    let num_tokens = owner_records.len();
    let response = tip004::BalanceOfResponse {
        num_tokens: num_tokens as u64,
    };
    let response_bytes = response.encode_to_vec();
    Ok(Some(response_bytes))
}

fn token_of_owner_by_index<TUnitOfWork: StateDbUnitOfWorkReader>(
    args: &[u8],
    state_db: &TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    // TODO: move this to the invoke_read_method method
    let request =
        tip004::TokenOfOwnerByIndexRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
            source: e,
            message_type: "tip004::TokenOfOwnerByIndex".to_string(),
        })?;

    let owner = request.owner.clone();
    let index = request.index;
    let owner_records = state_db.find_keys_by_value("owners", &owner)?;
    if let Some(token_id) = owner_records.into_iter().nth(index as usize) {
        let token = state_db
            .get_value("tokens", &token_id)?
            .ok_or_else(|| DigitalAssetError::NotFound {
                entity: "state_keys",
                id: format!("tokens.{}", token_id.to_hex()),
            })?;
        let response = tip004::TokenOfOwnerByIndexResponse {
            token_id,
            token: String::from_utf8(token).expect("should fix this"),
        };
        let response_bytes = response.encode_to_vec();
        Ok(Some(response_bytes))
    } else {
        Ok(None)
    }
}
