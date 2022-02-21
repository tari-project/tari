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

use log::*;
use prost::Message;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_dan_common_types::proto::tips::tip721;

use crate::{
    models::InstructionSet,
    storage::state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::dan_layer::core::templates::tip721_template";

pub fn initial_instructions(_: &TemplateParameter) -> InstructionSet {
    InstructionSet::empty()
}

pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
    method: &str,
    args: &[u8],
    state_db: &TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "ownerof" => {
            let request =
                tip721::OwnerOfRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
                    source: e,
                    message_type: "tip721::OwnerOfRequest".to_string(),
                })?;
            let response = tip721::OwnerOfResponse {
                owner: owner_of(request.token_id, state_db)?,
            };
            Ok(Some(response.encode_to_vec()))
        },
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
    method: &str,
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<(), DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "transferfrom" => transfer_from(args, state_db),
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

fn owner_of<TUnitOfWork: StateDbUnitOfWorkReader>(
    token_id: Vec<u8>,
    state_db: &TUnitOfWork,
) -> Result<Vec<u8>, DigitalAssetError> {
    state_db
        .get_value("owners", &token_id)?
        .ok_or_else(|| DigitalAssetError::NotFound {
            entity: "owner",
            id: token_id.to_hex(),
        })
}

fn transfer_from<TUnitOfWork: StateDbUnitOfWork>(
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<(), DigitalAssetError> {
    let request = tip721::TransferFromRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip721::TransferFromRequest".to_string(),
    })?;

    debug!(target: LOG_TARGET, "transfer_from called");
    let from = request.from.clone();
    let to = request.to.clone();
    let token_id = request.token_id;

    let owner = state_db
        .get_value("owners", &token_id)?
        .ok_or_else(|| DigitalAssetError::NotFound {
            entity: "owner",
            id: token_id.to_hex(),
        })?;
    if owner != from {
        return Err(DigitalAssetError::NotAuthorised(
            "Not authorized to send this address".to_string(),
        ));
    }
    // TODO: check signature

    state_db.set_value("owners".to_string(), token_id, to.to_vec())?;
    Ok(())
}
