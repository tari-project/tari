//  Copyright 2025, The Tari Project
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

use log::info;

use crate::chain_storage::{
    lmdb_db::migrations::manager::{Migration, MigrationStatus},
    LMDBDatabase,
};

pub const LOG_TARGET: &str = "c::cs::lmdb_db::lmdb_db::migrations";

/// MIGRATION: Accumulated difficulty migration after the 3rd mining algorithm was introduced
pub struct Migration03 {
    version: u64,
}

impl Migration03 {
    pub fn new() -> Self {
        Self { version: 3 }
    }
}

impl Migration for Migration03 {
    fn version(&self) -> u64 {
        self.version
    }

    fn run(&self, _db: Option<&mut LMDBDatabase>) -> MigrationStatus {
        info!(
            target: LOG_TARGET,
            "[MIGRATIONS] Starting migration v{}",
            self.version()
        );

        // Nothing to do here, this migration is a no-op.

        MigrationStatus {
            completed_version: Some(self.version()),
            in_progress_version: None,
            migration_error: None,
        }
    }

    fn requires_backend(&self) -> bool {
        false
    }
}
