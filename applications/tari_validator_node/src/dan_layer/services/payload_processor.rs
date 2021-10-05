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

use crate::{
    dan_layer::{
        models::{InstructionSet, Payload},
        services::{MempoolService, TemplateService},
    },
    digital_assets_error::DigitalAssetError,
};
use async_trait::async_trait;

#[async_trait]
pub trait PayloadProcessor<TPayload: Payload> {
    async fn process_payload(&mut self, payload: &TPayload) -> Result<(), DigitalAssetError>;
}

pub struct InstructionSetProcessor<TTemplateService: TemplateService, TMempoolService: MempoolService> {
    template_service: TTemplateService,
    mempool_service: TMempoolService,
}

impl<TTemplateService: TemplateService, TMempoolService: MempoolService>
    InstructionSetProcessor<TTemplateService, TMempoolService>
{
    pub fn new(template_service: TTemplateService, mempool_service: TMempoolService) -> Self {
        Self {
            template_service,
            mempool_service,
        }
    }
}

#[async_trait]
impl<TTemplateService: TemplateService + Send, TMempoolService: MempoolService + Send> PayloadProcessor<InstructionSet>
    for InstructionSetProcessor<TTemplateService, TMempoolService>
{
    async fn process_payload(&mut self, payload: &InstructionSet) -> Result<(), DigitalAssetError> {
        for instruction in payload.instructions() {
            dbg!("Executing instruction");
            dbg!(&instruction);
            // TODO: Should we swallow + log the error instead of propagating it?
            self.template_service.execute_instruction(instruction).await?;
        }

        self.mempool_service.remove_instructions(payload.instructions())?;

        Ok(())
    }
}
