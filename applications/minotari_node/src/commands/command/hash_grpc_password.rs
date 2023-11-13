//  Copyright 2023, The Tari Project
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

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use clap::Parser;
use minotari_app_grpc::authentication::salted_password::create_salted_hashed_password;

use super::{CommandContext, HandleCommand};

/// Hashes the GRPC authentication password from the config and returns an argon2 hash
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.hash_grpc_password().await
    }
}

impl CommandContext {
    pub async fn hash_grpc_password(&mut self) -> Result<(), Error> {
        match self
            .config
            .base_node
            .grpc_authentication
            .username_password()
            .ok_or_else(|| anyhow!("GRPC basic auth is not configured"))
        {
            Ok((username, password)) => {
                match create_salted_hashed_password(password.reveal()).map_err(|e| anyhow!(e.to_string())) {
                    Ok(hashed_password) => {
                        println!("Your hashed password is:");
                        println!("{}", *hashed_password);
                        println!();
                        println!(
                            "Use HTTP basic auth with username '{}' and the hashed password to make GRPC requests",
                            username
                        );
                    },
                    Err(e) => eprintln!("HashGrpcPassword error! {}", e),
                }
            },
            Err(e) => eprintln!("HashGrpcPassword error! {}", e),
        }

        Ok(())
    }
}
