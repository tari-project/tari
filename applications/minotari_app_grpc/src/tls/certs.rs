// Copyright 2023. The Tari Project
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
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use rcgen::{CertificateParams, DnType, IsCa::Ca, Issuer, KeyPair};

use crate::tls::error::GrpcTlsError;

pub fn generate_self_signed_certs() -> Result<(String, String, String), GrpcTlsError> {
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string(), "0.0.0.0".to_string()];
    let mut ca_params = CertificateParams::new(subject_alt_names.clone())?;
    ca_params.distinguished_name.push(DnType::CommonName, "127.0.0.1");
    ca_params.is_ca = Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate()?;
    let ca = ca_params.self_signed(&ca_key)?;
    let ca_issuer = Issuer::from_params(&ca_params, &ca_key);
    let cacert = ca.pem();

    let server_params = CertificateParams::new(subject_alt_names)?;
    let server_key = KeyPair::generate()?;
    let server_cert = server_params.signed_by(&server_key, &ca_issuer)?;

    Ok((cacert, server_cert.pem(), server_key.serialize_pem()))
}

pub fn write_cert_to_disk(dir: PathBuf, filename: &str, data: &String) -> Result<(), GrpcTlsError> {
    let path = dir.join(Path::new(filename));
    let mut file = File::create(&path)?;
    file.write_all(data.as_ref())?;

    println!("{path:?} written to disk.");

    Ok(())
}
pub fn print_warning() {
    println!(
        "⚠️WARNING: The use of self-signed TLS certificates poses a significant security risk. These certificates are \
         not issued or verified by a trusted Certificate Authority (CA), making them susceptible to man-in-the-middle \
         attacks. When employing self-signed certificates, the encryption provided is compromised, and your data may \
         be intercepted or manipulated without detection."
    );
    println!();
    println!(
        "It is strongly advised to use certificates issued by reputable CAs to ensure the authenticity and security \
         of your connections. Self-signed certificates are suitable for testing purposes only and should never be \
         used in a production environment where data integrity and confidentiality are paramount."
    );
    println!();
    println!(
        "Please exercise extreme caution and prioritize the use of valid, properly authenticated TLS certificates to \
         safeguard your applications and data against potential security threats."
    );
    println!();
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::{net::TcpListener, sync::oneshot};
    use tonic::{
        Request,
        Response,
        Status,
        transport::{Certificate, ClientTlsConfig, Endpoint, Identity, Server, ServerTlsConfig},
    };

    use super::*;
    use crate::tari_rpc::{
        GetNewBlockRequest,
        GetNewBlockResponse,
        GetTipInfoRequest,
        GetTipInfoResponse,
        SubmitBlockRequest,
        SubmitBlockResponse,
        sha_p2_pool_client::ShaP2PoolClient,
        sha_p2_pool_server::{ShaP2Pool, ShaP2PoolServer},
    };

    struct DummyP2Pool;

    #[tonic::async_trait]
    impl ShaP2Pool for DummyP2Pool {
        async fn get_tip_info(
            &self,
            _request: Request<GetTipInfoRequest>,
        ) -> Result<Response<GetTipInfoResponse>, Status> {
            Ok(Response::new(GetTipInfoResponse::default()))
        }

        async fn get_new_block(
            &self,
            _request: Request<GetNewBlockRequest>,
        ) -> Result<Response<GetNewBlockResponse>, Status> {
            Err(Status::unimplemented("unused in test"))
        }

        async fn submit_block(
            &self,
            _request: Request<SubmitBlockRequest>,
        ) -> Result<Response<SubmitBlockResponse>, Status> {
            Err(Status::unimplemented("unused in test"))
        }
    }

    #[tokio::test]
    async fn generates_valid_ca_and_server_tls_pem_bundle() {
        let (ca_pem, server_cert_pem, server_key_pem) = generate_self_signed_certs().unwrap();
        assert!(ca_pem.contains("BEGIN CERTIFICATE"));
        assert!(server_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(server_key_pem.contains("BEGIN PRIVATE KEY"));

        let identity = Identity::from_pem(server_cert_pem.as_bytes(), server_key_pem.as_bytes());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let incoming = tonic::transport::server::TcpIncoming::from(listener);
        let server_task = tokio::spawn(async move {
            let _unused = Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))
                .unwrap()
                .add_service(ShaP2PoolServer::new(DummyP2Pool))
                .serve_with_incoming_shutdown(incoming, async {
                    let _unused = shutdown_rx.await;
                })
                .await;
        });

        let url = format!("https://localhost:{}", addr.port());

        // 1. Client trusting `ca_pem` completes the TLS handshake and RPC call.
        let trusted_channel = Endpoint::from_shared(url.clone())
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(Certificate::from_pem(ca_pem.as_bytes()))
                    .domain_name("localhost"),
            )
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = ShaP2PoolClient::new(trusted_channel);
        client.get_tip_info(GetTipInfoRequest {}).await.unwrap();

        // 2. Client trusting an unrelated CA rejects the server certificate during the TLS handshake.
        let (unrelated_ca_pem, _, _) = generate_self_signed_certs().unwrap();
        let untrusted_result = Endpoint::from_shared(url)
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(Certificate::from_pem(unrelated_ca_pem.as_bytes()))
                    .domain_name("localhost"),
            )
            .unwrap()
            .connect()
            .await;
        assert!(
            untrusted_result.is_err(),
            "TLS handshake must fail when client trusts an unrelated CA"
        );

        let _unused = shutdown_tx.send(());
        let _unused = server_task.await;
    }
}
