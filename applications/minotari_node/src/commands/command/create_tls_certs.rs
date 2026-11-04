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
use minotari_app_grpc::tls::certs::{
    default_sans,
    generate_self_signed_certs,
    print_warning,
    renew_leaf,
    write_cert_to_disk,
    write_key_to_disk,
};

use super::{CommandContext, HandleCommand};

/// Create self signed TLS certificates for use with gRPC
#[derive(Debug, Parser)]
pub struct Args {
    /// Renew the leaf certificate while retaining the existing CA.
    #[clap(long)]
    pub renew_leaf: bool,
    /// DNS names or IP addresses for the leaf certificate.
    #[clap(long = "san")]
    pub sans: Vec<String>,
}

fn sans_or_default(sans: Vec<String>) -> Vec<String> {
    if sans.is_empty() { default_sans() } else { sans }
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.create_tls_certs(args)
    }
}

impl CommandContext {
    pub fn create_tls_certs(&self, args: Args) -> Result<(), Error> {
        let sans = sans_or_default(args.sans);
        if args.renew_leaf {
            renew_leaf(self.config.base_node.config_dir.clone(), sans)?;
            return Ok(());
        }
        match generate_self_signed_certs(sans) {
            Ok((cacert, ca_key, cert, private_key)) => {
                print_warning();

                write_cert_to_disk(self.config.base_node.config_dir.clone(), "node_ca.pem", &cacert)?;
                write_key_to_disk(self.config.base_node.config_dir.clone(), "node_ca.key", &ca_key)?;
                write_cert_to_disk(self.config.base_node.config_dir.clone(), "server.pem", &cert)?;
                write_key_to_disk(self.config.base_node.config_dir.clone(), "server.key", &private_key)?;

                println!();
                println!("Certificates generated successfully.");
                println!(
                    "To continue configuration move the `node_ca.pem` to the client service's `application/config/` \
                     directory. Restart the base node with the configuration grpc_tls_enabled=true"
                );
                println!();
            },
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repeated_sans_and_renewal() {
        let args = Args::try_parse_from(["create-tls-certs", "--renew-leaf", "--san", "one", "--san", "two"]).unwrap();
        assert!(args.renew_leaf);
        assert_eq!(args.sans, ["one", "two"]);
    }

    #[test]
    fn san_defaults_are_applied_only_when_empty() {
        assert_eq!(sans_or_default(Vec::new()), default_sans());
        assert_eq!(sans_or_default(vec!["custom".into()]), vec!["custom"]);
    }
}
