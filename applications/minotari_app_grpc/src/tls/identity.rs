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

use std::path::PathBuf;

use tonic::transport::Identity;

use crate::tls::error::GrpcTlsError;

#[cfg(unix)]
pub fn private_key_is_group_or_other_readable(mode: u32) -> bool {
    mode & 0o044 != 0
}

pub async fn read_identity(config_dir: PathBuf) -> Result<Identity, GrpcTlsError> {
    let err = |file| move |e| GrpcTlsError::FileReadError(format!("Could not load the file `{file:?}`: {e}"));

    let cert_file = config_dir.join("server.pem");
    let cert = tokio::fs::read(&cert_file).await.map_err(err(cert_file.clone()))?;

    // Check certificate expiry and warn or error accordingly
    if let Ok((_, x509_pem)) = x509_parser::pem::parse_x509_pem(&cert) &&
        let Ok(parsed_cert) = x509_pem.parse_x509()
    {
        let not_after = parsed_cert.validity().not_after.timestamp();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let remediation_hint = if config_dir.join("node_ca.key").exists() {
            "Run `minotari_node create-tls-certs --renew-leaf` to renew it."
        } else {
            "Delete the existing certificate files and re-run `create-tls-certs` to regenerate them."
        };
        if now >= not_after {
            return Err(GrpcTlsError::CertExpired(format!(
                "server.pem has expired. {remediation_hint}"
            )));
        }
        let remaining = not_after.saturating_sub(now);
        if remaining < 30 * 86_400 {
            let days_left = remaining / 86_400;
            log::warn!(
                "TLS server certificate {:?} will expire in {} days. {}",
                cert_file,
                days_left,
                remediation_hint
            );
        }
    }

    let key_file = config_dir.join("server.key");
    #[cfg(unix)]
    if let Ok(metadata) = tokio::fs::metadata(&key_file).await {
        use std::os::unix::fs::PermissionsExt;
        if private_key_is_group_or_other_readable(metadata.permissions().mode()) {
            log::warn!("TLS private key {:?} is readable by group or other users", key_file);
        }
    }
    let key = tokio::fs::read(&key_file).await.map_err(err(key_file))?;

    Ok(Identity::from_pem(cert, key))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn private_key_permission_predicate() {
        assert!(!super::private_key_is_group_or_other_readable(0o600));
        assert!(super::private_key_is_group_or_other_readable(0o644));
    }
}
