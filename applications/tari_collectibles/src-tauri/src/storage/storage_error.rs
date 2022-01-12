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

use tari_utilities::ByteArrayError;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
  #[error("Could not connect to database: {source}")]
  ConnectionError {
    #[from]
    source: diesel::ConnectionError,
  },
  #[error("General diesel error: {source}")]
  DieselError {
    #[from]
    source: diesel::result::Error,
  },
  #[error("Could not migrate the database: {source}")]
  MigrationError {
    #[from]
    source: diesel_migrations::RunMigrationsError,
  },
  #[error("UUID error: {source}")]
  UuidError {
    #[from]
    source: uuid::Error,
  },
  #[error("KeyManager error: {source}")]
  KeyManagerError {
    #[from]
    source: tari_key_manager::error::KeyManagerError,
  },
  #[error("The password is incorrect")]
  WrongPassword,
  #[error("Could not update value in database because another thread has already updated it. Table:{table}, old_value: {old_value}, new_value:{new_value}")]
  ConcurrencyError {
    table: &'static str,
    old_value: String,
    new_value: String,
  },
  #[error("Invalid struct stored as bytes")]
  ByteArrayError(#[from] ByteArrayError),
}
