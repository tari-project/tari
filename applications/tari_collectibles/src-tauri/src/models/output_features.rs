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
use crate::models::TemplateParameter;
use serde::{Deserialize, Serialize};
use tari_app_grpc::tari_rpc;

#[derive(Serialize, Deserialize)]
pub struct OutputFeatures {
  template_ids_implemented: Vec<u32>,
  template_parameters: Vec<TemplateParameter>,
}

impl From<tari_rpc::OutputFeatures> for OutputFeatures {
  fn from(v: tari_rpc::OutputFeatures) -> Self {
    let asset = v.asset.as_ref();
    Self {
      template_ids_implemented: asset
        .map(|f| f.template_ids_implemented.clone())
        .unwrap_or_default(),
      template_parameters: asset
        .map(|f| {
          f.template_parameters
            .iter()
            .map(|tp| TemplateParameter {
              template_id: tp.template_id,
              template_data_version: tp.template_data_version,
              template_data: tp.template_data.clone(),
            })
            .collect()
        })
        .unwrap_or_default(),
    }
  }
}
