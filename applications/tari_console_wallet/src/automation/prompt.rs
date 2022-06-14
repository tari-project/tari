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

pub struct Prompt<'a> {
    label: &'a str,
    skip_if_some: Option<String>,
    default: Option<String>,
}

impl<'a> Prompt<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            default: None,
            skip_if_some: None,
        }
    }

    pub fn skip_if_some(mut self, value: Option<String>) -> Self {
        self.skip_if_some = value;
        self
    }

    pub fn with_default(mut self, default: String) -> Self {
        self.default = Some(default);
        self
    }

    pub fn get_result_parsed<T>(self) -> Result<T, CommandError>
    where
        T: FromStr,
        T::Err: ToString,
    {
        let result = self.get_result()?;
        let parsed = result
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
            let prompt = Prompt::new(self.label);
            let result = prompt.get_result_parsed()?;
            collection.push(result);

            loop {
                println!("Add another? Y/N");
                print!("> ");

                io::stdout().flush()?;
                let mut line_buf = String::new();
                io::stdin().read_line(&mut line_buf)?;
                println!();

                let trimmed = line_buf.trim();

                match trimmed {
                    "N" => {
                        return Ok(collection);
                    },
                    "Y" => break,
                    _ => continue,
                }
            }
        }
    }

    pub fn get_result(self) -> Result<String, CommandError> {
        if let Some(value) = self.skip_if_some {
            return Ok(value);
        }
        loop {
            match self.default {
                Some(ref default) => {
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
                    Some(ref default) => return Ok(default.clone()),
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
