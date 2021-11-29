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
use crate::{models::AssetDefinition, storage::state::StateDbUnitOfWork, DigitalAssetError};
use prost::Message;
use tari_core::transactions::transaction::TemplateParameter;
use tari_crypto::tari_utilities::ByteArray;
use tari_dan_common_types::proto::tips::tip002;

pub fn init<TUnitOfWork: StateDbUnitOfWork>(
    template_parameter: &TemplateParameter,
    asset_definition: &AssetDefinition,
    state_db: &mut TUnitOfWork,
) -> Result<(), DigitalAssetError> {
    let params = tip002::InitRequest::decode(&*template_parameter.template_data).map_err(|e| {
        DigitalAssetError::ProtoBufDecodeError {
            source: e,
            message_type: "tip002::InitRequest".to_string(),
        }
    })?;
    dbg!(&params);
    state_db.set_value(
        "owners".to_string(),
        asset_definition.public_key.to_vec(),
        Vec::from(params.total_supply.to_le_bytes()),
    )?;
    Ok(())
}

pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWork>(
    method: String,
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    match method.as_str() {
        "BalanceOf" => balance_of(args, state_db),
        _ => todo!(),
    }
}

fn balance_of<TUnitOfWork: StateDbUnitOfWork>(
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    let request = tip002::BalanceOfRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip002::BalanceOfRequest".to_string(),
    })?;

    let data = state_db.get_value("owners", &request.owner)?;
    match data {
        Some(data) => {
            let mut data2: [u8; 8] = [0; 8];
            data2.copy_from_slice(&data);

            let balance = u64::from_le_bytes(data2);
            let response = tip002::BalanceOfResponse { balance };
            let mut output_buffer = vec![];
            response
                .encode(&mut output_buffer)
                .map_err(|e| DigitalAssetError::ProtoBufEncodeError {
                    source: e,
                    message_type: "tip002::BalanceOfResponse".to_string(),
                })?;

            Ok(Some(output_buffer))
        },
        None => Ok(None),
    }
}
