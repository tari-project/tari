// Copyright 2020. The Tari Project
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

use std::{
    fs,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};

use crate::automation::{
    commands::{FILE_EXTENSION, SESSION_INFO},
    error::CommandError,
    Step1SessionInfo,
};

#[derive(Debug)]
pub(crate) struct PartialRead {
    pub(crate) lines_to_read: usize,
    pub(crate) lines_to_skip: usize,
}

/// Reads an entire file into a single JSON object
pub(crate) fn json_from_file_single_object<P: AsRef<Path>, T: DeserializeOwned>(
    path: P,
    partial_read: Option<PartialRead>,
) -> Result<T, CommandError> {
    if let Some(val) = partial_read {
        let lines = BufReader::new(
            File::open(path.as_ref())
                .map_err(|e| CommandError::JsonFile(format!("{e} '{}'", path.as_ref().display())))?,
        )
        .lines()
        .take(val.lines_to_read)
        .skip(val.lines_to_skip);
        let mut json_str = String::new();
        for line in lines {
            let line = line.map_err(|e| CommandError::JsonFile(format!("{e} '{}'", path.as_ref().display())))?;
            json_str.push_str(&line);
        }
        serde_json::from_str(&json_str)
            .map_err(|e| CommandError::JsonFile(format!("{e} '{}'", path.as_ref().display())))
    } else {
        serde_json::from_reader(BufReader::new(
            File::open(path.as_ref())
                .map_err(|e| CommandError::JsonFile(format!("{e} '{}'", path.as_ref().display())))?,
        ))
        .map_err(|e| CommandError::JsonFile(format!("{e} '{}'", path.as_ref().display())))
    }
}

/// Write a single JSON object to file as a single line
pub(crate) fn write_json_object_to_file_as_line<T: Serialize>(
    file: &Path,
    reset_file: bool,
    outputs: T,
) -> Result<(), CommandError> {
    if let Some(file_path) = file.parent() {
        if !file_path.exists() {
            fs::create_dir_all(file_path).map_err(|e| CommandError::JsonFile(format!("{} ({})", e, file.display())))?;
        }
    }
    if reset_file && file.exists() {
        fs::remove_file(file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    }
    append_json_line_to_file(file, outputs)?;
    Ok(())
}

fn append_json_line_to_file<P: AsRef<Path>, T: Serialize>(file: P, output: T) -> Result<(), CommandError> {
    fs::create_dir_all(file.as_ref().parent().unwrap()).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let mut file_object = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let json = serde_json::to_string(&output).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    writeln!(file_object, "{json}").map_err(|e| CommandError::JsonFile(e.to_string()))?;
    Ok(())
}

/// Write outputs to a JSON file
pub(crate) fn write_to_json_file<T: Serialize>(file: &Path, reset_file: bool, data: T) -> Result<(), CommandError> {
    if let Some(file_path) = file.parent() {
        if !file_path.exists() {
            fs::create_dir_all(file_path).map_err(|e| CommandError::JsonFile(format!("{} ({})", e, file.display())))?;
        }
    }
    if reset_file && file.exists() {
        fs::remove_file(file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    }
    append_to_json_file(file, data)?;
    Ok(())
}

fn append_to_json_file<P: AsRef<Path>, T: Serialize>(file: P, data: T) -> Result<(), CommandError> {
    fs::create_dir_all(file.as_ref().parent().unwrap()).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let mut file_object = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let json = serde_json::to_string_pretty(&data).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    writeln!(file_object, "{json}").map_err(|e| CommandError::JsonFile(e.to_string()))?;
    Ok(())
}

/// Return the output directory for the session
pub(crate) fn out_dir(session_id: &str) -> Result<PathBuf, CommandError> {
    let base_dir = dirs::cache_dir().ok_or(CommandError::InvalidArgument(
        "Could not find cache directory".to_string(),
    ))?;
    Ok(base_dir.join("tari_faucets").join(session_id))
}

/// Move the session file to the session directory
pub(crate) fn move_session_file_to_session_dir(session_id: &str, input_file: &PathBuf) -> Result<(), CommandError> {
    let out_dir = out_dir(session_id)?;
    let session_file = out_dir.join(get_file_name(SESSION_INFO, None));
    if input_file != &session_file {
        fs::copy(input_file.clone(), session_file.clone())?;
        fs::remove_file(input_file.clone())?;
        println!(
            "Session info file '{}' moved to '{}'",
            input_file.display(),
            session_file.display()
        );
    }
    Ok(())
}

/// Read the session info from the session directory
pub(crate) fn read_session_info(
    session_id: &str,
    session_file: Option<PathBuf>,
) -> Result<Step1SessionInfo, CommandError> {
    let file_path = if let Some(file) = session_file {
        file
    } else {
        out_dir(session_id)?.join(get_file_name(SESSION_INFO, None))
    };
    let session_info = json_from_file_single_object::<_, Step1SessionInfo>(&file_path, None)?;
    if session_info.session_id != session_id {
        return Err(CommandError::InvalidArgument(format!(
            "Session ID in session info file '{}' mismatch",
            get_file_name(SESSION_INFO, None)
        )));
    }
    Ok(session_info)
}

/// Read the inputs from the session directory and verify the header
pub(crate) fn read_and_verify<T: DeserializeOwned>(
    session_id: &str,
    file_name: &str,
    session_info: &Step1SessionInfo,
) -> Result<T, CommandError> {
    let out_dir = out_dir(session_id)?;
    let header = json_from_file_single_object::<_, Step1SessionInfo>(
        &out_dir.join(file_name),
        Some(PartialRead {
            lines_to_read: 1,
            lines_to_skip: 0,
        }),
    )?;
    if session_id != header.session_id {
        return Err(CommandError::InvalidArgument(format!(
            "Session ID in header for file '{}' mismatch",
            file_name
        )));
    }
    if session_info != &header {
        return Err(CommandError::InvalidArgument(format!(
            "Session info in header for file '{}' mismatch",
            file_name
        )));
    }
    json_from_file_single_object::<_, T>(
        &out_dir.join(file_name),
        Some(PartialRead {
            lines_to_read: usize::MAX,
            lines_to_skip: 1,
        }),
    )
}

/// Create the file name with the given stem and optional suffix
pub(crate) fn get_file_name(stem: &str, suffix: Option<String>) -> String {
    let mut file_name = stem.to_string();
    if let Some(suffix) = suffix {
        file_name.push_str(&suffix);
    }
    file_name.push('.');
    file_name.push_str(FILE_EXTENSION);
    file_name
}
