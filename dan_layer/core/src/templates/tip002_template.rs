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
use crate::{
    models::AssetDefinition,
    storage::state::StateDbUnitOfWork,
    templates::proto::tips::tip002,
    DigitalAssetError,
};
use prost::Message;
use tari_core::transactions::transaction::TemplateParameter;
use tari_crypto::tari_utilities::ByteArray;

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
    );
    Ok(())
}
