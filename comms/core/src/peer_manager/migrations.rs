// Copyright 2020, The Tari Project
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

mod v7;

use log::*;
use tari_storage::lmdb_store::{LMDBDatabase, LMDBError};

const LOG_TARGET: &str = "comms::peer_manager::migrations";

pub(super) const MIGRATION_VERSION_KEY: u64 = u64::MAX;

pub fn migrate(database: &LMDBDatabase) -> Result<(), LMDBError> {
    // Add migrations here in version order
    let migrations = [v7::Migration.boxed()];
    if migrations.is_empty() {
        return Ok(());
    }
    let latest_version = migrations.last().unwrap().get_version();

    // If the database is empty there is nothing to migrate, so set it to the latest version
    if database.len()? == 0 {
        debug!(target: LOG_TARGET, "New database does not require migration");
        if let Err(err) = database.insert(&MIGRATION_VERSION_KEY, &latest_version) {
            error!(
                target: LOG_TARGET,
                "Failed to update migration counter: {}. ** Database may be corrupt **", err
            );
        }
        return Ok(());
    }

    let mut version = database.get::<_, u32>(&MIGRATION_VERSION_KEY)?.unwrap_or(0);

    if version == latest_version {
        debug!(
            target: LOG_TARGET,
            "Database at version {}. No migration required.", latest_version
        );
        return Ok(());
    }

    debug!(
        target: LOG_TARGET,
        "Migrating database from version {} to {}", version, latest_version
    );

    loop {
        version += 1;
        let migration = migrations.iter().find(|m| m.get_version() == version);
        match migration {
            Some(migration) => {
                migration.migrate(database)?;
                if let Err(err) = database.insert(&MIGRATION_VERSION_KEY, &version) {
                    error!(
                        target: LOG_TARGET,
                        "Failed to update migration counter: {}. ** Database may be corrupt **", err
                    );
                }

                debug!(target: LOG_TARGET, "Migration {} complete", version);
            },
            None => {
                if version - 1 != latest_version {
                    error!(
                        target: LOG_TARGET,
                        "Migration {} not found. Unable to migrate peer db", version
                    );
                }
                return Ok(());
            },
        }
    }
}

trait Migration<T> {
    type Error;

    fn get_version(&self) -> u32;

    fn migrate(&self, db: &T) -> Result<(), Self::Error>;
}

trait MigrationExt<T>: Migration<T> {
    fn boxed(self) -> Box<dyn Migration<T, Error = Self::Error>>
    where Self: Sized + 'static {
        Box::new(self)
    }
}

impl<T, U> MigrationExt<T> for U where U: Migration<T> {}
