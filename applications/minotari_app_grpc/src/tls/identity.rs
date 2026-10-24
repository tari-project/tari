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
const LOG_TARGET: &str = "minotari::grpc::tls";

/// Returns `true` if `mode` grants any group or world permissions (`mode & 0o077 != 0`).
pub const fn key_mode_is_group_or_world_accessible(mode: u32) -> bool {
    mode & 0o077 != 0
}

pub async fn read_identity(config_dir: PathBuf) -> Result<Identity, GrpcTlsError> {
    let err = |file| move |e| GrpcTlsError::FileReadError(format!("Could not load the file `{file:?}`: {e}"));

    let cert_file = config_dir.join("server.pem");
    let cert = tokio::fs::read(&cert_file).await.map_err(err(cert_file))?;

    let key_file = config_dir.join("server.key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = tokio::fs::metadata(&key_file).await.map_err(err(key_file.clone()))?;
        let mode = metadata.permissions().mode();
        if key_mode_is_group_or_world_accessible(mode) {
            log::warn!(
                target: LOG_TARGET,
                "TLS private key `{:?}` has overly permissive file mode {:04o}; group/other permissions should not be set (expected 0600)",
                key_file,
                mode & 0o777,
            );
        }
    }
    let key = tokio::fs::read(&key_file).await.map_err(err(key_file))?;

    Ok(Identity::from_pem(cert, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::certs::{generate_self_signed_certs, write_cert_to_disk, write_key_to_disk};

    #[test]
    fn key_mode_permission_check_detects_group_or_world_bits() {
        assert!(!key_mode_is_group_or_world_accessible(0o100600));
        assert!(!key_mode_is_group_or_world_accessible(0o100400));
        assert!(key_mode_is_group_or_world_accessible(0o100644));
        assert!(key_mode_is_group_or_world_accessible(0o100640));
        assert!(key_mode_is_group_or_world_accessible(0o100604));
        assert!(key_mode_is_group_or_world_accessible(0o100666));
    }

    #[tokio::test]
    async fn read_identity_loads_cert_and_key_and_warns_when_world_readable() {
        #[cfg(unix)]
        let warnings = {
            use std::sync::{Mutex, OnceLock};

            struct TestLogger {
                messages: Mutex<Vec<String>>,
            }

            impl log::Log for TestLogger {
                fn enabled(&self, metadata: &log::Metadata) -> bool {
                    metadata.level() <= log::Level::Warn
                }

                fn log(&self, record: &log::Record) {
                    if self.enabled(record.metadata()) {
                        self.messages.lock().unwrap().push(format!("{}", record.args()));
                    }
                }

                fn flush(&self) {}
            }

            static LOGGER: OnceLock<TestLogger> = OnceLock::new();
            let logger = LOGGER.get_or_init(|| TestLogger {
                messages: Mutex::new(Vec::new()),
            });
            let _ = log::set_logger(logger);
            log::set_max_level(log::LevelFilter::Warn);
            logger.messages.lock().unwrap().clear();
            logger
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let (_ca_cert, _ca_key, cert, key) = generate_self_signed_certs().unwrap();
        write_cert_to_disk(temp_dir.path().to_path_buf(), "server.pem", &cert).unwrap();
        write_key_to_disk(temp_dir.path().to_path_buf(), "server.key", &key, false).unwrap();

        let _identity = read_identity(temp_dir.path().to_path_buf()).await.unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert!(
                warnings
                    .messages
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|msg| !msg.contains("server.key")),
                "0600 key should not emit a permission warning"
            );

            let key_path = temp_dir.path().join("server.key");
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();

            let _identity = read_identity(temp_dir.path().to_path_buf()).await.unwrap();
            let logged = warnings.messages.lock().unwrap();
            assert!(
                logged
                    .iter()
                    .any(|msg| msg.contains("server.key") && msg.contains("0644")),
                "expected warning for 0644 server.key, got: {:?}",
                *logged
            );
        }
    }
}
