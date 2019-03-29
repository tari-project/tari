// Copyright 2019 The Tari Project
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

use derive_error::Error;
use serde::de::DeserializeOwned;
use std::{fs::File, io::prelude::*};

// TODO: file should be decrypted using Salsa20 or ChaCha20

#[derive(Debug, Error)]
pub enum FileError {
    // The specified backup file could not be created
    FileCreate,
    // The specified backup file could not be opened
    FileOpen,
    // Could not read from backup file
    FileRead,
    // Could not write to backup file
    FileWrite,
    // Problem serializing struct into JSON
    Serialize,
    // Problem deserializing JSON into a new struct
    Deserialize,
}

pub trait FileBackup<T> {
    fn from_file(filename: &String) -> Result<T, FileError>;
    fn to_file(&self, filename: &String) -> Result<(), FileError>;
}

impl<T> FileBackup<T> for T
where T: serde::Serialize + DeserializeOwned
{
    /// Load struct state from backup file
    fn from_file(filename: &String) -> Result<T, FileError> {
        let mut file_handle = match File::open(&filename) {
            Ok(file) => file,
            Err(_e) => return Err(FileError::FileOpen),
        };
        let mut file_content = String::new();
        match file_handle.read_to_string(&mut file_content) {
            Ok(_) => match serde_json::from_str(&file_content) {
                Ok(km) => Ok(km),
                Err(_) => Err(FileError::Deserialize),
            },
            Err(_) => Err(FileError::FileRead),
        }
    }

    /// Backup struct state in file specified by filename
    fn to_file(&self, filename: &String) -> Result<(), FileError> {
        match File::create(filename) {
            Ok(mut file_handle) => match serde_json::to_string(&self) {
                Ok(json_data) => match file_handle.write_all(json_data.as_bytes()) {
                    Ok(_) => Ok(()),
                    Err(_) => Err(FileError::FileWrite),
                },
                Err(_) => Err(FileError::Serialize),
            },

            Err(_) => Err(FileError::FileCreate),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::file_backup::*;
    use serde_derive::{Deserialize, Serialize};
    use std::fs::remove_file;

    #[test]
    fn test_struct_to_file_and_from_file() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        pub struct R {
            pub var1: String,
            pub var2: Vec<u8>,
            pub var3: usize,
        }
        let desired_struct = R {
            var1: "Test".to_string(),
            var2: vec![0, 1, 2],
            var3: 3,
        };
        // Backup struct to file
        let backup_filename = "test_backup.json".to_string();
        match desired_struct.to_file(&backup_filename) {
            Ok(_v) => {
                // Restore struct from file
                let backup_result: Result<R, FileError> = R::from_file(&backup_filename);
                match backup_result {
                    Ok(backup_struct) => {
                        // Remove temp backup file
                        remove_file(backup_filename).unwrap();
                        assert_eq!(desired_struct, backup_struct);
                    },
                    Err(_e) => assert!(false),
                };
            },
            Err(_e) => assert!(false),
        };
    }
}
