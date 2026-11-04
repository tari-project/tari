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

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

use rcgen::{
    BasicConstraints,
    CertificateParams,
    DnType,
    ExtendedKeyUsagePurpose,
    IsCa::Ca,
    Issuer,
    KeyPair,
    KeyUsagePurpose,
};
use time::{Duration, OffsetDateTime};

use crate::tls::error::GrpcTlsError;

pub fn default_sans() -> Vec<String> {
    vec!["localhost".into(), "127.0.0.1".into(), "0.0.0.0".into()]
}

pub fn generate_self_signed_certs(
    subject_alt_names: Vec<String>,
) -> Result<(String, String, String, String), GrpcTlsError> {
    validate_sans(&subject_alt_names)?;
    let now = OffsetDateTime::now_utc();
    let mut ca_params = CertificateParams::new(subject_alt_names.clone())?;
    if let Some(common_name) = subject_alt_names.first() {
        ca_params.distinguished_name.push(DnType::CommonName, common_name);
    }
    ca_params.not_before = now;
    ca_params.not_after = now + Duration::days(365 * 10);
    ca_params.is_ca = Ca(BasicConstraints::Constrained(0));
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_key = KeyPair::generate()?;
    let ca = ca_params.self_signed(&ca_key)?;
    let ca_issuer = Issuer::from_params(&ca_params, &ca_key);
    let cacert = ca.pem();

    let mut server_params = CertificateParams::new(subject_alt_names)?;
    server_params.not_before = now;
    server_params.not_after = now + Duration::days(365 * 2);
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server_key = KeyPair::generate()?;
    let server_cert = server_params.signed_by(&server_key, &ca_issuer)?;

    Ok((
        cacert,
        ca_key.serialize_pem(),
        server_cert.pem(),
        server_key.serialize_pem(),
    ))
}

pub fn write_cert_to_disk(dir: PathBuf, filename: &str, data: &str) -> Result<(), GrpcTlsError> {
    let path = dir.join(filename);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o644);
    let mut file = options.open(&path)?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    file.write_all(data.as_bytes())?;

    println!("{path:?} written to disk.");

    Ok(())
}

pub fn write_key_to_disk(dir: PathBuf, filename: &str, data: &str) -> Result<(), GrpcTlsError> {
    let path = dir.join(filename);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path)?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(data.as_bytes())?;
    println!("{path:?} written to disk.");
    Ok(())
}

pub fn generate_leaf_from_ca(
    ca_pem: &str,
    ca_key_pem: &str,
    subject_alt_names: Vec<String>,
) -> Result<(String, String), GrpcTlsError> {
    validate_sans(&subject_alt_names)?;
    let ca_key = KeyPair::from_pem(&ca_key_pem)?;
    let issuer = Issuer::from_ca_cert_pem(&ca_pem, ca_key)?;
    let now = OffsetDateTime::now_utc();
    let mut params = CertificateParams::new(subject_alt_names)?;
    params.not_before = now;
    params.not_after = now + Duration::days(365 * 2);
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let key = KeyPair::generate()?;
    let cert = params.signed_by(&key, &issuer)?;
    Ok((cert.pem(), key.serialize_pem()))
}

fn validate_sans(sans: &[String]) -> Result<(), rcgen::Error> {
    for san in sans {
        if san.is_empty() || san.len() > 253 || san.split('.').any(|label| label.is_empty() || label.len() > 63) {
            return Err(rcgen::Error::InvalidNameType);
        }
    }
    Ok(())
}

pub fn renew_leaf(dir: PathBuf, subject_alt_names: Vec<String>) -> Result<(), GrpcTlsError> {
    let ca_pem = std::fs::read_to_string(dir.join("node_ca.pem"))?;
    let ca_key_pem = std::fs::read_to_string(dir.join("node_ca.key"))?;
    let (cert, key) = generate_leaf_from_ca(&ca_pem, &ca_key_pem, subject_alt_names)?;
    let suffix = format!("renewal.{}", std::process::id());
    let cert_tmp = dir.join(format!("server.pem.{suffix}"));
    let key_tmp = dir.join(format!("server.key.{suffix}"));
    let result = (|| {
        write_temp(&cert_tmp, &cert, false)?;
        write_temp(&key_tmp, &key, true)?;
        std::fs::rename(cert_tmp.as_path(), dir.join("server.pem"))?;
        std::fs::rename(key_tmp.as_path(), dir.join("server.key"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(cert_tmp);
        let _ = std::fs::remove_file(key_tmp);
    }
    result
}

fn write_temp(path: &std::path::Path, data: &str, private: bool) -> Result<(), GrpcTlsError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, if private { 0o600 } else { 0o644 });
    let mut file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(if private { 0o600 } else { 0o644 }))?;
    file.write_all(data.as_bytes())?;
    file.sync_all()?;
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
    use std::{fs, net::SocketAddr};

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

    #[test]
    fn invalid_san_is_a_certificate_generation_error() {
        assert!(matches!(
            generate_self_signed_certs(vec![format!("{}.com", "a".repeat(64))]),
            Err(GrpcTlsError::CertGenerationError(_))
        ));
    }

    #[test]
    fn generated_certificates_have_expected_x509_properties() {
        use x509_parser::{
            extensions::{GeneralName, ParsedExtension},
            pem::parse_x509_pem,
        };
        let (ca_pem, _, leaf_pem, _) =
            generate_self_signed_certs(vec!["grpc.example.com".into(), "10.0.0.5".into()]).unwrap();
        let (_, ca) = parse_x509_pem(ca_pem.as_bytes()).unwrap();
        let (_, leaf) = parse_x509_pem(leaf_pem.as_bytes()).unwrap();
        let ca = ca.parse_x509().unwrap();
        let leaf = leaf.parse_x509().unwrap();
        let ca_duration = ca.validity().not_after.timestamp() - ca.validity().not_before.timestamp();
        let leaf_duration = leaf.validity().not_after.timestamp() - leaf.validity().not_before.timestamp();
        assert!((ca_duration - 3650 * 86_400).abs() <= 2);
        assert!((leaf_duration - 730 * 86_400).abs() <= 2);
        let ca_exts = ca.extensions().iter().map(|e| e.parsed_extension()).collect::<Vec<_>>();
        let bc = ca_exts
            .iter()
            .find_map(|e| match e {
                ParsedExtension::BasicConstraints(v) => Some(v),
                _ => None,
            })
            .unwrap();
        assert!(bc.ca && bc.path_len_constraint == Some(0));
        let ku = ca_exts
            .iter()
            .find_map(|e| match e {
                ParsedExtension::KeyUsage(v) => Some(v),
                _ => None,
            })
            .unwrap();
        assert!(ku.key_cert_sign() && ku.crl_sign());
        let eku = leaf
            .extensions()
            .iter()
            .find_map(|e| match e.parsed_extension() {
                ParsedExtension::ExtendedKeyUsage(v) => Some(v),
                _ => None,
            })
            .unwrap();
        assert!(eku.server_auth);
        let san = leaf
            .extensions()
            .iter()
            .find_map(|e| match e.parsed_extension() {
                ParsedExtension::SubjectAlternativeName(v) => Some(v),
                _ => None,
            })
            .unwrap();
        assert!(
            san.general_names
                .iter()
                .any(|n| matches!(n, GeneralName::DNSName(name) if *name == "grpc.example.com"))
        );
        assert!(
            san.general_names
                .iter()
                .any(|n| matches!(n, GeneralName::IPAddress(bytes) if *bytes == [10, 0, 0, 5]))
        );
    }

    #[test]
    fn writers_are_non_overwriting_and_use_secure_modes() {
        let dir = tempfile::tempdir().unwrap();
        write_cert_to_disk(dir.path().to_path_buf(), "server.pem", "cert").unwrap();
        write_key_to_disk(dir.path().to_path_buf(), "server.key", "key").unwrap();
        write_cert_to_disk(dir.path().to_path_buf(), "node_ca.pem", "ca cert").unwrap();
        write_key_to_disk(dir.path().to_path_buf(), "node_ca.key", "ca key").unwrap();
        assert_eq!(fs::read(dir.path().join("server.pem")).unwrap(), b"cert");
        assert_eq!(fs::read(dir.path().join("server.key")).unwrap(), b"key");
        assert!(write_cert_to_disk(dir.path().to_path_buf(), "server.pem", "changed").is_err());
        assert!(write_key_to_disk(dir.path().to_path_buf(), "server.key", "changed").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("server.pem"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o644
            );
            assert_eq!(
                fs::metadata(dir.path().join("server.key"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o600
            );
        }
    }

    #[test]
    fn renewal_preserves_ca_and_signs_custom_leaf() {
        let dir = tempfile::tempdir().unwrap();
        let (ca, ca_key, cert, key) =
            generate_self_signed_certs(vec!["custom.example".into(), "10.0.0.1".into()]).unwrap();
        write_cert_to_disk(dir.path().to_path_buf(), "node_ca.pem", &ca).unwrap();
        write_key_to_disk(dir.path().to_path_buf(), "node_ca.key", &ca_key).unwrap();
        write_cert_to_disk(dir.path().to_path_buf(), "server.pem", &cert).unwrap();
        write_key_to_disk(dir.path().to_path_buf(), "server.key", &key).unwrap();
        renew_leaf(dir.path().to_path_buf(), vec!["renewed.example".into()]).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("node_ca.pem")).unwrap(), ca);
        assert_eq!(fs::read_to_string(dir.path().join("node_ca.key")).unwrap(), ca_key);
        assert_ne!(fs::read_to_string(dir.path().join("server.pem")).unwrap(), cert);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("server.key"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(dir.path().join("node_ca.pem"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o644
            );
            assert_eq!(
                fs::metadata(dir.path().join("node_ca.key"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(dir.path().join("server.pem"))
                    .unwrap()
                    .permissions()
                    .mode() &
                    0o777,
                0o644
            );
        }
    }

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
        let (ca_pem, _, server_cert_pem, server_key_pem) = generate_self_signed_certs(default_sans()).unwrap();
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
        let (unrelated_ca_pem, _, _, _) = generate_self_signed_certs(default_sans()).unwrap();
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
