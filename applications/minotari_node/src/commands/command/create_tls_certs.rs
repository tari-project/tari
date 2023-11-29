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

use anyhow::Error;
use async_trait::async_trait;
use clap::Parser;
use minotari_app_grpc::tls::certs::{generate_self_signed_certs, print_warning, write_cert_to_disk};

use super::{CommandContext, HandleCommand};

/// Create self signed TLS certificates for use with gRPC
#[derive(Debug, Parser)]
pub struct Args {}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, _: Args) -> Result<(), Error> {
        self.create_tls_certs()
    }
}

impl CommandContext {
    pub fn create_tls_certs(&self) -> Result<(), Error> {
        match generate_self_signed_certs() {
            Ok((cacert, cert, private_key)) => {
                print_warning();

                write_cert_to_disk(self.config.base_node.config_dir.clone(), "node_ca.pem", &cacert)?;
                write_cert_to_disk(self.config.base_node.config_dir.clone(), "server.pem", &cert)?;
                write_cert_to_disk(self.config.base_node.config_dir.clone(), "server.key", &private_key)?;

                println!("Certificates generated successfully.");
                println!(
                    "To continue configuration move the `node_ca.pem` to the client service's `application/config/` \
                     directory."
                );
            },
            Err(err) => eprintln!("Error generating certificates: {}", err),
        }
        Ok(())
    }
}
