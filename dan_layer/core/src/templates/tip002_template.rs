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

use prost::Message;
use tari_core::transactions::transaction_components::TemplateParameter;
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_dan_common_types::proto::tips::tip002;

use crate::{
    models::{Instruction, InstructionSet, TemplateId},
    storage::state::{StateDbUnitOfWork, StateDbUnitOfWorkReader},
    DigitalAssetError,
};

pub fn initial_instructions(template_param: &TemplateParameter) -> InstructionSet {
    InstructionSet::from_vec(vec![Instruction::new(
        TemplateId::Tip002,
        "init".to_string(),
        template_param.template_data.clone(),
    )])
}

pub fn invoke_read_method<TUnitOfWork: StateDbUnitOfWorkReader>(
    method: &str,
    args: &[u8],
    state_db: &TUnitOfWork,
) -> Result<Option<Vec<u8>>, DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "balanceof" => balance_of(args, state_db),
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

pub fn invoke_write_method<TUnitOfWork: StateDbUnitOfWork>(
    method: &str,
    args: &[u8],
    state_db: &mut TUnitOfWork,
) -> Result<(), DigitalAssetError> {
    match method.to_lowercase().replace("_", "").as_str() {
        "init" => init(args, state_db),
        "transfer" => transfer(args, state_db),
        name => Err(DigitalAssetError::TemplateUnsupportedMethod { name: name.to_string() }),
    }
}

fn init<TUnitOfWork: StateDbUnitOfWork>(args: &[u8], state_db: &mut TUnitOfWork) -> Result<(), DigitalAssetError> {
    let params = tip002::InitRequest::decode(args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip002::InitRequest".to_string(),
    })?;
    dbg!(&params);
    state_db.set_value(
        "owners".to_string(),
        state_db.context().asset_public_key().to_vec(),
        // TODO: Encode full owner data
        Vec::from(params.total_supply.to_le_bytes()),
    )?;
    Ok(())
}

fn balance_of<TUnitOfWork: StateDbUnitOfWorkReader>(
    args: &[u8],
    state_db: &TUnitOfWork,
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

fn transfer<TUnitOfWork: StateDbUnitOfWork>(args: &[u8], state_db: &mut TUnitOfWork) -> Result<(), DigitalAssetError> {
    let request = tip002::TransferRequest::decode(&*args).map_err(|e| DigitalAssetError::ProtoBufDecodeError {
        source: e,
        message_type: "tip002::TransferRequest".to_string(),
    })?;

    dbg!(&request);
    let data = state_db.get_value("owners", &request.from)?;
    match data {
        Some(data) => {
            let mut data2: [u8; 8] = [0; 8];
            data2.copy_from_slice(&data);
            let balance = u64::from_le_bytes(data2);
            if balance < request.amount {
                return Err(DigitalAssetError::NotEnoughFunds);
            }
            let new_balance = balance - request.amount;
            dbg!(new_balance);
            state_db.set_value(
                "owners".to_string(),
                request.from.clone(),
                Vec::from(new_balance.to_le_bytes()),
            )?;
            let receiver_data = state_db.get_value("owners", &request.to)?;
            let mut receiver_balance = match receiver_data {
                Some(d) => {
                    let mut data2: [u8; 8] = [0; 8];
                    data2.copy_from_slice(&d);

                    u64::from_le_bytes(data2)
                },
                None => 0,
            };
            dbg!(receiver_balance);
            receiver_balance = receiver_balance
                .checked_add(request.amount)
                .ok_or(DigitalAssetError::Overflow)?;
            dbg!(receiver_balance);
            state_db.set_value(
                "owners".to_string(),
                request.to,
                Vec::from(receiver_balance.to_le_bytes()),
            )?;
            Ok(())
        },
        None => Err(DigitalAssetError::NotFound {
            entity: "address",
            id: request.from.to_hex(),
        }),
    }
}
