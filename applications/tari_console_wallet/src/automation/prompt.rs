//  Copyright 2022, The Tari Project
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

use std::{io, io::Write, str::FromStr};

use tari_utilities::hex::{Hex, HexError};

use crate::automation::error::CommandError;
pub struct Prompt {
    label: String,
    skip_if_some: Option<String>,
    default: Option<String>,
}

impl Prompt {
    pub fn new<T: Into<String>>(label: T) -> Self {
        Self {
            label: label.into(),
            default: None,
            skip_if_some: None,
        }
    }

    pub fn skip_if_some(mut self, value: Option<String>) -> Self {
        self.skip_if_some = value;
        self
    }

    pub fn with_default<T: Into<String>>(mut self, default: T) -> Self {
        self.default = Some(default.into());
        self
    }

    pub fn ask_parsed<T>(self) -> Result<T, CommandError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        let resp = self.ask()?;
        let parsed = resp
            .parse()
            .map_err(|e: T::Err| CommandError::InvalidArgument(e.to_string()))?;
        Ok(parsed)
    }

    pub fn ask_repeatedly<T>(self) -> Result<Vec<T>, CommandError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        let mut collection: Vec<T> = vec![];

        loop {
            let prompt = Prompt::new(self.label.clone());
            let result = prompt.ask_parsed()?;
            collection.push(result);

            loop {
                println!("Add another? Y/N");
                print!("> ");

                io::stdout().flush()?;
                let mut line_buf = String::new();
                io::stdin().read_line(&mut line_buf)?;
                println!();

                let trimmed = line_buf.trim().to_lowercase();
                match trimmed.as_str() {
                    "n" => {
                        return Ok(collection);
                    },
                    "y" => break,
                    _ => continue,
                }
            }
        }
    }

    pub fn ask(self) -> Result<String, CommandError> {
        if let Some(value) = self.skip_if_some {
            return Ok(value);
        }
        loop {
            match self.default.as_ref().filter(|s| !s.is_empty()) {
                Some(default) => {
                    println!("{} (Default: {})", self.label, default);
                },
                None => {
                    println!("{}", self.label);
                },
            }
            print!("> ");
            io::stdout().flush()?;
            let mut line_buf = String::new();
            io::stdin().read_line(&mut line_buf)?;
            println!();
            let trimmed = line_buf.trim();
            if trimmed.is_empty() {
                match self.default {
                    Some(default) => return Ok(default),
                    None => continue,
                }
            } else {
                return Ok(trimmed.to_string());
            }
        }
    }
}

pub struct HexArg<T>(T);

impl<T> HexArg<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Hex> FromStr for HexArg<T> {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(T::from_hex(s)?))
    }
}

pub struct Optional<T>(Option<T>);

impl<T> Optional<T> {
    pub fn into_inner(self) -> Option<T> {
        self.0
    }
}

impl<T: FromStr> FromStr for Optional<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            Ok(Self(None))
        } else {
            Ok(Self(Some(T::from_str(s)?)))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct YesNo(bool);

impl YesNo {
    pub fn as_bool(self) -> bool {
        self.0
    }
}

impl FromStr for YesNo {
    type Err = CommandError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(Self(true)),
            "n" | "no" => Ok(Self(false)),
            _ => Err(CommandError::Argument),
        }
    }
}
