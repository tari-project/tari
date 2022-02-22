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

use crate::{error::CollectiblesError, storage::StorageError};
use diesel::result::Error;
use prost::{DecodeError, EncodeError};
use serde::{Deserialize, Serialize};
use tari_utilities::hex::HexError;

#[derive(Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Status {
  BadRequest {
    code: u16,
    message: String,
  },
  Unauthorized {
    code: u16,
    message: String,
  },
  NotFound {
    code: u16,
    message: String,
    entity: String,
  },
  Internal {
    code: u16,
    message: String,
  },
}

impl Status {
  pub fn unauthorized() -> Self {
    Self::Unauthorized {
      code: 401,
      message: "Unauthorized".to_string(),
    }
  }

  pub fn internal(message: String) -> Self {
    Self::Internal { code: 500, message }
  }
  pub fn not_found(entity: String) -> Self {
    Self::NotFound {
      code: 404,
      message: format!("{} not found", &entity),
      entity,
    }
  }
}

impl From<StorageError> for Status {
  fn from(source: StorageError) -> Self {
    match source {
      StorageError::DieselError { source } => match source {
        Error::NotFound => Self::NotFound {
          code: 404,
          message: format!("Not found: {}", source),
          entity: "Unknown".to_string(),
        },
        _ => Self::Internal {
          code: 502,
          message: format!("Internal diesel storage error: {}", source),
        },
      },
      _ => Self::Internal {
        code: 501,
        message: format!("Internal storage error: {}", source),
      },
    }
  }
}

impl From<HexError> for Status {
  fn from(he: HexError) -> Self {
    Self::BadRequest {
      code: 400,
      message: format!("Bad request: {}", he),
    }
  }
}

impl From<DecodeError> for Status {
  fn from(de: DecodeError) -> Self {
    Self::Internal {
      code: 502,
      message: format!("Could not decode data: {}", de),
    }
  }
}

impl From<EncodeError> for Status {
  fn from(e: EncodeError) -> Self {
    Self::Internal {
      code: 503,
      message: format!("Could not encode data: {}", e),
    }
  }
}

impl From<CollectiblesError> for Status {
  fn from(ce: CollectiblesError) -> Self {
    Self::Internal {
      code: 504,
      message: format!("Error: {}", ce),
    }
  }
}
