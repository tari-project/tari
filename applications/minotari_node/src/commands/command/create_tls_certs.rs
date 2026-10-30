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
    generate_self_signed_certs_with_sans_and_validity,
    print_warning,
    renew_leaf_cert_from_ca_files_with_validity,
    write_ca_and_server_certs_to_disk,
    write_cert_and_key_pair_to_disk,
};

use super::{CommandContext, HandleCommand};

/// Create self signed TLS certificates for use with gRPC
#[derive(Debug, Default, Clone, Parser)]
pub struct Args {
    /// Subject Alternative Names (DNS names or IP addresses) to include in the certificate.
    /// Defaults to `localhost`, `127.0.0.1`, and `0.0.0.0` when omitted.
    #[clap(long = "san")]
    pub san: Vec<String>,
    /// Optional leaf certificate validity period in days (defaults to no expiration, `9999-12-31`, when omitted).
    #[clap(long)]
    pub validity_days: Option<u64>,
    /// Optional CA certificate validity period in days (defaults to no expiration, `9999-12-31`, when omitted;
    /// clamped to at least `--validity-days`).
    #[clap(long)]
    pub ca_validity_days: Option<u64>,
    /// Regenerate and sign only `server.pem` and `server.key` using the existing `node_ca.pem` and `node_ca.key`
    #[clap(long)]
    pub renew_leaf: bool,
    /// Overwrite existing private keys on disk
    #[clap(long, short)]
    pub force: bool,
}

#[async_trait]
impl HandleCommand<Args> for CommandContext {
    async fn handle_command(&mut self, args: Args) -> Result<(), Error> {
        self.create_tls_certs(args)
    }
}

impl CommandContext {
    pub fn create_tls_certs(&self, args: Args) -> Result<(), Error> {
        let config_dir = self.config.base_node.config_dir.clone();
        if args.renew_leaf {
            let (cert, private_key) = renew_leaf_cert_from_ca_files_with_validity(
                &config_dir,
                "node_ca.pem",
                "node_ca.key",
                &args.san,
                args.validity_days,
            )?;
            print_warning();
            write_cert_and_key_pair_to_disk(&config_dir, "server.pem", &cert, "server.key", &private_key, true)?;

            println!();
            println!("Leaf certificate renewed successfully.");
            println!();
            return Ok(());
        }

        let (cacert, ca_key, cert, private_key) =
            generate_self_signed_certs_with_sans_and_validity(&args.san, args.validity_days, args.ca_validity_days)?;
        print_warning();

        write_ca_and_server_certs_to_disk(
            &config_dir,
            "node_ca.pem",
            &cacert,
            "node_ca.key",
            &ca_key,
            "server.pem",
            &cert,
            "server.key",
            &private_key,
            args.force,
        )?;

        println!();
        println!("Certificates generated successfully.");
        println!(
            "To continue configuration copy the `node_ca.pem` to the client service's `application/config/` \
             directory. Restart the base node with the configuration grpc_tls_enabled=true"
        );
        println!();
        Ok(())
    }
}
