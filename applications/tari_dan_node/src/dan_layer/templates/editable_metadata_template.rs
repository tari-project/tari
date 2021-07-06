// Copyright 2021. The Tari Project
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

use crate::dan_layer::template_command::{TemplateCommand, ExecutionResult};
use crate::digital_assets_error::DigitalAssetError;
use crate::dan_layer::asset_data_store::AssetDataStore;
use crate::dan_layer::template_service::InstructionCaller;
use crate::dan_layer::TokenId;

pub struct EditableMetadataTemplate {

}

impl EditableMetadataTemplate {
    pub fn create_command(method : String, args: Vec<Vec<u8>>, caller: InstructionCaller) -> Result<impl TemplateCommand, DigitalAssetError> {
 match method.as_str() {
     "update" => {
         let token_id = caller.owner_token_id().clone();
         let metadata = args.first().ok_or_else(|| DigitalAssetError::MissingArgument{argument_name: "metadata".to_string(), position: 0})?;
         // TODO: check for too many args

         Ok(UpdateMetadataCommand::new(token_id, metadata.clone(), caller))
     },
     _ => Err(DigitalAssetError::UnknownMethod{ method_name: method.clone()})
     }
    }
}
pub struct UpdateMetadataCommand {
    token_id: TokenId,
    metadata: Vec<u8>,
    caller: InstructionCaller
}

impl UpdateMetadataCommand {
   pub fn new(token_id: TokenId, metadata: Vec<u8>,  caller: InstructionCaller) -> Self {
       Self {
           token_id, metadata,
           caller
       }
   }
}
impl TemplateCommand for UpdateMetadataCommand {
    fn try_execute(&self, data_store: &mut AssetDataStore) -> Result<ExecutionResult, DigitalAssetError> {
        // TODO: put in concurrency check, perhaps nonce?
        data_store.replace_metadata(&self.token_id, self.metadata.clone())?;
        Ok(ExecutionResult::Ok)
    }
}
