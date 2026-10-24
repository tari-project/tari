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
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rcgen::{
    BasicConstraints,
    Certificate,
    CertificateParams,
    DnType,
    ExtendedKeyUsagePurpose,
    IsCa,
    KeyPair,
    KeyUsagePurpose,
    date_time_ymd,
};
use time::OffsetDateTime;
use yasna::models::ObjectIdentifier;

use crate::tls::error::GrpcTlsError;

/// Default Subject Alternative Names included when `--san` is not supplied.
pub const DEFAULT_TLS_SANS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0"];

/// Leaf certificate validity period (2 years).
pub const LEAF_CERT_VALIDITY: Duration = Duration::from_secs(2 * 365 * 24 * 60 * 60);

/// CA certificate validity period (10 years).
pub const CA_CERT_VALIDITY: Duration = Duration::from_secs(10 * 365 * 24 * 60 * 60);

const OID_COMMON_NAME: &[u64] = &[2, 5, 4, 3];

fn current_utc_second() -> OffsetDateTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    date_time_ymd(1970, 1, 1) + Duration::from_secs(secs)
}

fn resolve_sans(subject_alt_names: &[String]) -> Vec<String> {
    if subject_alt_names.is_empty() {
        DEFAULT_TLS_SANS.iter().map(|s| (*s).to_string()).collect()
    } else {
        subject_alt_names.to_vec()
    }
}

fn build_ca_params(subject_alt_names: &[String], common_name: &str, not_before: OffsetDateTime) -> CertificateParams {
    let mut params = CertificateParams::new(subject_alt_names.to_vec());
    params.distinguished_name.push(DnType::CommonName, common_name);
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = not_before;
    params.not_after = not_before + CA_CERT_VALIDITY;
    params
}

fn build_leaf_params(subject_alt_names: &[String], not_before: OffsetDateTime) -> CertificateParams {
    let mut params = CertificateParams::new(subject_alt_names.to_vec());
    params
        .distinguished_name
        .push(DnType::CommonName, subject_alt_names.join(", "));
    params.is_ca = IsCa::ExplicitNoCa;
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = not_before;
    params.not_after = not_before + LEAF_CERT_VALIDITY;
    params
}

pub(crate) fn generate_certs_from_params(
    ca_params: CertificateParams,
    leaf_params: CertificateParams,
) -> Result<(String, String, String, String), GrpcTlsError> {
    let ca = Certificate::from_params(ca_params)?;
    let cacert = ca.serialize_pem()?;
    let ca_key = ca.serialize_private_key_pem();

    let server_cert = Certificate::from_params(leaf_params)?;
    let cert = server_cert.serialize_pem_with_signer(&ca)?;
    let private_key = server_cert.serialize_private_key_pem();

    Ok((cacert, ca_key, cert, private_key))
}

/// Generates a constrained self-signed CA certificate (10-year validity, `pathlen:0`, `KeyCertSign` + `CrlSign`) and a
/// server leaf certificate (2-year validity, `ServerAuth`) using the default SAN set (`localhost`, `127.0.0.1`,
/// `0.0.0.0`).
///
/// Returns `(ca_cert_pem, ca_key_pem, server_cert_pem, server_key_pem)`.
pub fn generate_self_signed_certs() -> Result<(String, String, String, String), GrpcTlsError> {
    generate_self_signed_certs_with_sans(&[])
}

/// Generates a constrained self-signed CA certificate and a server leaf certificate for the provided Subject
/// Alternative Names (falling back to [`DEFAULT_TLS_SANS`] when `subject_alt_names` is empty).
///
/// Returns `(ca_cert_pem, ca_key_pem, server_cert_pem, server_key_pem)`.
pub fn generate_self_signed_certs_with_sans(
    subject_alt_names: &[String],
) -> Result<(String, String, String, String), GrpcTlsError> {
    let sans = resolve_sans(subject_alt_names);
    let not_before = current_utc_second();
    let common_name = sans.join(", ");
    let ca_params = build_ca_params(&sans, &common_name, not_before);
    let leaf_params = build_leaf_params(&sans, not_before);
    generate_certs_from_params(ca_params, leaf_params)
}

/// Regenerates a fresh leaf certificate and private key signed by the existing CA certificate and private key,
/// verifying that the CA certificate's public key matches `ca_key_pem` and preserving the CA certificate's Subject
/// CommonName so the leaf's Issuer matches the CA Subject even if `--san` differs from when the CA was originally
/// created.
pub fn renew_leaf_cert(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    subject_alt_names: &[String],
) -> Result<(String, String), GrpcTlsError> {
    renew_leaf_cert_with_ca_cert(ca_cert_pem, ca_key_pem, subject_alt_names)
}

/// Regenerates a fresh leaf certificate and private key signed by the existing CA certificate and private key,
/// verifying that the CA certificate's public key matches `ca_key_pem` and preserving the CA certificate's Subject
/// CommonName so the leaf's Issuer matches the CA Subject even if `--san` differs from when the CA was originally
/// created.
pub fn renew_leaf_cert_with_ca_cert(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    subject_alt_names: &[String],
) -> Result<(String, String), GrpcTlsError> {
    let sans = resolve_sans(subject_alt_names);
    let ca_key_pair = KeyPair::from_pem(ca_key_pem)?;
    let (ca_cn, ca_spki_der) = extract_subject_cn_and_spki_from_pem(ca_cert_pem)?;
    if ca_spki_der != ca_key_pair.public_key_der() {
        return Err(GrpcTlsError::CaKeyMismatch);
    }
    renew_leaf_cert_with_ca_key_pair(ca_key_pair, &ca_cn, &sans)
}

fn renew_leaf_cert_with_ca_key_pair(
    ca_key_pair: KeyPair,
    ca_common_name: &str,
    sans: &[String],
) -> Result<(String, String), GrpcTlsError> {
    let not_before = current_utc_second();
    let mut ca_params = build_ca_params(sans, ca_common_name, not_before);
    ca_params.key_pair = Some(ca_key_pair);
    let ca = Certificate::from_params(ca_params)?;

    let leaf_params = build_leaf_params(sans, not_before);
    let server_cert = Certificate::from_params(leaf_params)?;
    let cert_pem = server_cert.serialize_pem_with_signer(&ca)?;
    let key_pem = server_cert.serialize_private_key_pem();

    Ok((cert_pem, key_pem))
}

/// Reads the persisted CA certificate and CA private key from `dir`, verifies that they form a matching key pair, and
/// signs a fresh leaf certificate and private key.
pub fn renew_leaf_cert_from_ca_files(
    dir: &Path,
    ca_cert_filename: &str,
    ca_key_filename: &str,
    subject_alt_names: &[String],
) -> Result<(String, String), GrpcTlsError> {
    let ca_key_path = dir.join(ca_key_filename);
    let ca_key_pem = fs::read_to_string(&ca_key_path)
        .map_err(|e| GrpcTlsError::FileReadError(format!("Could not load the file `{ca_key_path:?}`: {e}")))?;

    let ca_cert_path = dir.join(ca_cert_filename);
    let ca_cert_pem = fs::read_to_string(&ca_cert_path)
        .map_err(|e| GrpcTlsError::FileReadError(format!("Could not load the file `{ca_cert_path:?}`: {e}")))?;

    renew_leaf_cert_with_ca_cert(&ca_cert_pem, &ca_key_pem, subject_alt_names)
}

fn extract_subject_cn_and_spki_from_pem(cert_pem: &str) -> Result<(String, Vec<u8>), GrpcTlsError> {
    let parsed_pem = pem::parse(cert_pem).map_err(|_| rcgen::Error::CouldNotParseCertificate)?;
    if parsed_pem.tag() != "CERTIFICATE" {
        return Err(rcgen::Error::CouldNotParseCertificate.into());
    }
    let cn_oid = ObjectIdentifier::from_slice(OID_COMMON_NAME);

    let (subject_cn, spki_der) = yasna::parse_der(parsed_pem.contents(), |reader| {
        reader.read_sequence(|reader| {
            let tbs = reader.next().read_sequence(|reader| {
                // version [0] EXPLICIT
                let _ = reader.next().read_der()?;
                // serialNumber
                let _ = reader.next().read_der()?;
                // signature AlgorithmIdentifier
                let _ = reader.next().read_der()?;
                // issuer Name
                let _ = reader.next().read_der()?;
                // validity
                let _ = reader.next().read_der()?;
                // subject Name
                let cn = read_common_name(reader.next(), &cn_oid)?;
                // subjectPublicKeyInfo
                let spki_der = reader.next().read_der()?;
                // optional extensions
                let _ = reader.read_optional(|r| r.read_der())?;
                Ok((cn, spki_der))
            })?;
            // signatureAlgorithm
            let _ = reader.next().read_der()?;
            // signatureValue
            let _ = reader.next().read_der()?;
            Ok(tbs)
        })
    })
    .map_err(|_| GrpcTlsError::from(rcgen::Error::CouldNotParseCertificate))?;

    if subject_cn.is_empty() {
        return Err(rcgen::Error::CouldNotParseCertificate.into());
    }

    Ok((subject_cn, spki_der))
}

fn read_common_name(reader: yasna::BERReader<'_, '_>, cn_oid: &ObjectIdentifier) -> Result<String, yasna::ASN1Error> {
    let rdns = reader.collect_sequence_of(|reader| {
        reader.collect_set_of(|reader| {
            reader.read_sequence(|reader| {
                let oid = reader.next().read_oid()?;
                let val = reader.next().read_utf8string()?;
                Ok((oid, val))
            })
        })
    })?;
    for rdn in rdns {
        for (oid, val) in rdn {
            if &oid == cn_oid {
                return Ok(val);
            }
        }
    }
    Ok(String::new())
}

#[derive(Clone, Copy)]
struct StagedTlsFile<'a> {
    filename: &'a str,
    data: &'a str,
    is_key: bool,
}

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_staged_temp_file(dir: &Path, entry: StagedTlsFile<'_>, index: usize) -> Result<PathBuf, io::Error> {
    let pid = std::process::id();
    let seq = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".tls_stage_{pid}_{seq}_{index}_{}", entry.filename);
    let temp_path = dir.join(temp_name);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if entry.is_key { 0o600 } else { 0o644 });
    }
    let mut file = options.open(&temp_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(if entry.is_key { 0o600 } else { 0o644 }))?;
    }
    if let Err(err) = file.write_all(entry.data.as_bytes()).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    Ok(temp_path)
}

fn write_target_file_in_place(path: &Path, data: &[u8], is_key: bool, force: bool) -> Result<(), io::Error> {
    let mut options = OpenOptions::new();
    options.write(true);
    if !is_key || force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if is_key { 0o600 } else { 0o644 });
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(if is_key { 0o600 } else { 0o644 }))?;
    }
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn restore_backups(committed_paths: &[PathBuf], backups: &[(PathBuf, Option<(Vec<u8>, fs::Permissions, bool)>)]) {
    for path in committed_paths {
        if let Some((_, backup_opt)) = backups.iter().find(|(p, _)| p == path) {
            match backup_opt {
                Some((old_bytes, old_perms, is_key)) => {
                    if write_target_file_in_place(path, old_bytes, *is_key, true).is_ok() {
                        let _ = fs::set_permissions(path, old_perms.clone());
                    }
                },
                None => {
                    let _ = fs::remove_file(path);
                },
            }
        }
    }
}

fn write_staged_tls_files(dir: &Path, entries: &[StagedTlsFile<'_>], force: bool) -> Result<(), GrpcTlsError> {
    // 1. Check `!force` for private keys before touching any file on disk.
    for entry in entries {
        let target_path = dir.join(entry.filename);
        if entry.is_key && !force && fs::symlink_metadata(&target_path).is_ok() {
            return Err(GrpcTlsError::IoError(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("File `{target_path:?}` already exists"),
            )));
        }
    }

    // 2. Stage all new files into temporary files in `dir`.
    let mut staged_paths = Vec::with_capacity(entries.len());
    for (idx, entry) in entries.iter().copied().enumerate() {
        match create_staged_temp_file(dir, entry, idx) {
            Ok(temp_path) => staged_paths.push(temp_path),
            Err(err) => {
                for p in &staged_paths {
                    let _ = fs::remove_file(p);
                }
                return Err(GrpcTlsError::IoError(err));
            },
        }
    }

    // 3. Snapshot any existing target files so we can roll back if any subsequent write fails.
    let mut backups: Vec<(PathBuf, Option<(Vec<u8>, fs::Permissions, bool)>)> = Vec::with_capacity(entries.len());
    for entry in entries {
        let target_path = dir.join(entry.filename);
        if fs::symlink_metadata(&target_path).is_ok() {
            let snapshot = fs::metadata(&target_path).and_then(|meta| {
                let bytes = fs::read(&target_path)?;
                Ok((bytes, meta.permissions(), entry.is_key))
            });
            match snapshot {
                Ok(data) => backups.push((target_path, Some(data))),
                Err(err) => {
                    for p in &staged_paths {
                        let _ = fs::remove_file(p);
                    }
                    return Err(GrpcTlsError::IoError(err));
                },
            }
        } else {
            backups.push((target_path, None));
        }
    }

    // 4. Write all target files from staged contents; if any write fails, restore all previously modified files.
    let mut committed_paths: Vec<PathBuf> = Vec::with_capacity(entries.len());
    for (entry, staged_path) in entries.iter().zip(staged_paths.iter()) {
        let target_path = dir.join(entry.filename);
        let write_res = fs::read(staged_path)
            .and_then(|staged_bytes| write_target_file_in_place(&target_path, &staged_bytes, entry.is_key, force));
        let _ = fs::remove_file(staged_path);
        match write_res {
            Ok(()) => committed_paths.push(target_path),
            Err(err) => {
                for remaining in &staged_paths {
                    let _ = fs::remove_file(remaining);
                }
                restore_backups(&committed_paths, &backups);
                return Err(GrpcTlsError::IoError(err));
            },
        }
    }

    for path in &committed_paths {
        println!("{path:?} written to disk.");
    }

    Ok(())
}

/// Stages and writes a certificate and private key pair to `dir`, preserving any existing pair if either write fails.
pub fn write_cert_and_key_pair_to_disk(
    dir: &Path,
    cert_filename: &str,
    cert_pem: &str,
    key_filename: &str,
    key_pem: &str,
    force: bool,
) -> Result<(), GrpcTlsError> {
    let entries = [
        StagedTlsFile {
            filename: key_filename,
            data: key_pem,
            is_key: true,
        },
        StagedTlsFile {
            filename: cert_filename,
            data: cert_pem,
            is_key: false,
        },
    ];
    write_staged_tls_files(dir, &entries, force)
}

/// Stages and writes the CA and server certificate/key bundle to `dir`, preserving any existing files if any write
/// fails.
#[allow(clippy::too_many_arguments)]
pub fn write_ca_and_server_certs_to_disk(
    dir: &Path,
    ca_cert_filename: &str,
    ca_cert_pem: &str,
    ca_key_filename: &str,
    ca_key_pem: &str,
    server_cert_filename: &str,
    server_cert_pem: &str,
    server_key_filename: &str,
    server_key_pem: &str,
    force: bool,
) -> Result<(), GrpcTlsError> {
    let entries = [
        StagedTlsFile {
            filename: server_key_filename,
            data: server_key_pem,
            is_key: true,
        },
        StagedTlsFile {
            filename: ca_key_filename,
            data: ca_key_pem,
            is_key: true,
        },
        StagedTlsFile {
            filename: ca_cert_filename,
            data: ca_cert_pem,
            is_key: false,
        },
        StagedTlsFile {
            filename: server_cert_filename,
            data: server_cert_pem,
            is_key: false,
        },
    ];
    write_staged_tls_files(dir, &entries, force)
}

/// Writes a public TLS certificate artifact (`.pem`) to `dir/filename`.
///
/// On Unix, the file mode is explicitly set to `0644`.
pub fn write_cert_to_disk(dir: PathBuf, filename: &str, data: &str) -> Result<(), GrpcTlsError> {
    let path = dir.join(Path::new(filename));
    write_target_file_in_place(&path, data.as_bytes(), false, true)?;
    println!("{path:?} written to disk.");
    Ok(())
}

/// Writes a private TLS key artifact (`.key`) to `dir/filename`.
///
/// Opens with `create_new(true)` unless `force` is `true` (in which case it truncates and overwrites).
/// On Unix, the file mode is explicitly set to `0600`. On Windows, restricting the configuration directory ACL is the
/// operator's responsibility.
pub fn write_key_to_disk(dir: PathBuf, filename: &str, data: &str, force: bool) -> Result<(), GrpcTlsError> {
    let path = dir.join(Path::new(filename));
    write_target_file_in_place(&path, data.as_bytes(), true, force)?;
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
    use std::{
        convert::Infallible,
        io,
        net::{IpAddr, Ipv4Addr},
        task::{Context, Poll},
    };

    use rcgen::SanType;
    use tonic::{
        body::Body,
        codegen::{BoxFuture, Service, http},
        server::NamedService,
        transport::{
            Certificate as TonicCertificate,
            ClientTlsConfig,
            Endpoint,
            Identity,
            Server,
            ServerTlsConfig,
            server::TcpIncoming,
        },
    };
    use yasna::{Tag, tags::TAG_UTCTIME};

    use super::*;

    const OID_KEY_USAGE: &[u64] = &[2, 5, 29, 15];
    const OID_SUBJECT_ALT_NAME: &[u64] = &[2, 5, 29, 17];
    const OID_BASIC_CONSTRAINTS: &[u64] = &[2, 5, 29, 19];
    const OID_EXT_KEY_USAGE: &[u64] = &[2, 5, 29, 37];
    const OID_KP_SERVER_AUTH: &[u64] = &[1, 3, 6, 1, 5, 5, 7, 3, 1];

    #[derive(Debug)]
    struct ParsedX509 {
        issuer_cn: String,
        subject_cn: String,
        not_before: OffsetDateTime,
        not_after: OffsetDateTime,
        basic_constraints: Option<(bool, Option<u8>)>,
        key_usage_critical: bool,
        key_cert_sign: bool,
        crl_sign: bool,
        ext_key_usages: Vec<Vec<u64>>,
        sans: Vec<SanType>,
    }

    fn parse_x509_pem(cert_pem: &str) -> ParsedX509 {
        let parsed = pem::parse(cert_pem).expect("valid PEM");
        let cn_oid = ObjectIdentifier::from_slice(OID_COMMON_NAME);
        let ku_oid = ObjectIdentifier::from_slice(OID_KEY_USAGE);
        let san_oid = ObjectIdentifier::from_slice(OID_SUBJECT_ALT_NAME);
        let bc_oid = ObjectIdentifier::from_slice(OID_BASIC_CONSTRAINTS);
        let eku_oid = ObjectIdentifier::from_slice(OID_EXT_KEY_USAGE);

        yasna::parse_der(parsed.contents(), |reader| {
            reader.read_sequence(|reader| {
                let tbs = reader.next().read_sequence(|reader| {
                    // version [0] EXPLICIT u8
                    let version = reader.next().read_tagged(Tag::context(0), |r| r.read_u8())?;
                    assert_eq!(version, 2);
                    // serialNumber
                    let _ = reader.next().read_der()?;
                    // signature
                    let _ = reader.next().read_der()?;
                    // issuer
                    let issuer_cn = read_common_name(reader.next(), &cn_oid)?;
                    // validity
                    let (not_before, not_after) = reader.next().read_sequence(|reader| {
                        let nb = read_asn1_time(reader.next())?;
                        let na = read_asn1_time(reader.next())?;
                        Ok((nb, na))
                    })?;
                    // subject
                    let subject_cn = read_common_name(reader.next(), &cn_oid)?;
                    // subjectPublicKeyInfo
                    let _ = reader.next().read_der()?;
                    // extensions [3] EXPLICIT
                    let exts: Vec<(ObjectIdentifier, bool, Vec<u8>)> = reader
                        .read_optional(|r| {
                            r.read_tagged(Tag::context(3), |r| {
                                r.collect_sequence_of(|r| {
                                    r.read_sequence(|r| {
                                        let oid = r.next().read_oid()?;
                                        let critical = r.read_optional(|r| r.read_bool())?.unwrap_or(false);
                                        let value = r.next().read_bytes()?;
                                        Ok((oid, critical, value))
                                    })
                                })
                            })
                        })?
                        .unwrap_or_default();

                    let mut basic_constraints = None;
                    let mut key_usage_critical = false;
                    let mut key_cert_sign = false;
                    let mut crl_sign = false;
                    let mut ext_key_usages = Vec::new();
                    let mut sans = Vec::new();

                    for (oid, critical, value) in exts {
                        if oid == bc_oid {
                            basic_constraints = Some(yasna::parse_der(&value, |r| {
                                r.read_sequence(|r| {
                                    let is_ca = r.read_optional(|r| r.read_bool())?.unwrap_or(false);
                                    let path_len = r.read_optional(|r| r.read_u8())?;
                                    Ok((is_ca, path_len))
                                })
                            })?);
                        } else if oid == ku_oid {
                            key_usage_critical = critical;
                            let (bytes, _bit_len) = yasna::parse_der(&value, |r| r.read_bitvec_bytes())?;
                            let first_byte = bytes.first().copied().unwrap_or(0);
                            // ASN.1 BIT STRING is MSB-first: bit 5 = keyCertSign, bit 6 = cRLSign
                            key_cert_sign = (first_byte & (1 << (7 - 5))) != 0;
                            crl_sign = (first_byte & (1 << (7 - 6))) != 0;
                        } else if oid == eku_oid {
                            ext_key_usages = yasna::parse_der(&value, |r| {
                                r.collect_sequence_of(|r| Ok(r.read_oid()?.components().to_vec()))
                            })?;
                        } else if oid == san_oid {
                            sans = yasna::parse_der(&value, |r| {
                                r.collect_sequence_of(|r| {
                                    let tag = r.lookahead_tag()?;
                                    if tag == Tag::context(2) {
                                        let dns = r.read_tagged_implicit(Tag::context(2), |r| r.read_ia5_string())?;
                                        Ok(SanType::DnsName(dns))
                                    } else if tag == Tag::context(7) {
                                        let octets = r.read_tagged_implicit(Tag::context(7), |r| r.read_bytes())?;
                                        if let Ok(arr) = <[u8; 4]>::try_from(octets.as_slice()) {
                                            Ok(SanType::IpAddress(IpAddr::V4(Ipv4Addr::from(arr))))
                                        } else if let Ok(arr) = <[u8; 16]>::try_from(octets.as_slice()) {
                                            Ok(SanType::IpAddress(IpAddr::V6(std::net::Ipv6Addr::from(arr))))
                                        } else {
                                            Err(yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid))
                                        }
                                    } else {
                                        Err(yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid))
                                    }
                                })
                            })?;
                        }
                    }

                    Ok(ParsedX509 {
                        issuer_cn,
                        subject_cn,
                        not_before,
                        not_after,
                        basic_constraints,
                        key_usage_critical,
                        key_cert_sign,
                        crl_sign,
                        ext_key_usages,
                        sans,
                    })
                })?;
                // signatureAlgorithm
                let _ = reader.next().read_der()?;
                // signatureValue
                let _ = reader.next().read_der()?;
                Ok(tbs)
            })
        })
        .expect("valid X.509 DER")
    }

    fn read_asn1_time(reader: yasna::BERReader<'_, '_>) -> Result<OffsetDateTime, yasna::ASN1Error> {
        if reader.lookahead_tag()? == TAG_UTCTIME {
            Ok(*reader.read_utctime()?.datetime())
        } else {
            Ok(*reader.read_generalized_time()?.datetime())
        }
    }

    #[derive(Clone)]
    struct OkService;

    impl NamedService for OkService {
        const NAME: &'static str = "tari.rpc.TestTls";
    }

    impl Service<http::Request<Body>> for OkService {
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Infallible>;
        type Response = http::Response<Body>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: http::Request<Body>) -> Self::Future {
            Box::pin(async move { Ok(http::Response::new(Body::empty())) })
        }
    }

    #[test]
    fn write_key_and_cert_permissions_and_non_overwrite_without_force() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path().to_path_buf();

        let (ca_cert, ca_key, server_cert, server_key) = generate_self_signed_certs().unwrap();

        write_key_to_disk(dir.clone(), "server.key", &server_key, false).unwrap();
        write_key_to_disk(dir.clone(), "node_ca.key", &ca_key, false).unwrap();
        write_cert_to_disk(dir.clone(), "node_ca.pem", &ca_cert).unwrap();
        write_cert_to_disk(dir.clone(), "server.pem", &server_cert).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let server_key_mode = fs::metadata(dir.join("server.key")).unwrap().permissions().mode() & 0o777;
            let ca_key_mode = fs::metadata(dir.join("node_ca.key")).unwrap().permissions().mode() & 0o777;
            let ca_cert_mode = fs::metadata(dir.join("node_ca.pem")).unwrap().permissions().mode() & 0o777;
            let server_cert_mode = fs::metadata(dir.join("server.pem")).unwrap().permissions().mode() & 0o777;

            assert_eq!(server_key_mode, 0o600);
            assert_eq!(ca_key_mode, 0o600);
            assert_eq!(ca_cert_mode, 0o644);
            assert_eq!(server_cert_mode, 0o644);
        }

        // Second invocation without `--force` must fail and leave the existing key untouched.
        let err = write_key_to_disk(dir.clone(), "server.key", "OVERWRITTEN", false).unwrap_err();
        match err {
            GrpcTlsError::IoError(io_err) => assert_eq!(io_err.kind(), io::ErrorKind::AlreadyExists),
            other => panic!("expected GrpcTlsError::IoError(AlreadyExists), got: {other:?}"),
        }
        assert_eq!(fs::read_to_string(dir.join("server.key")).unwrap(), server_key);

        // With `force = true`, overwriting succeeds.
        write_key_to_disk(dir.clone(), "server.key", "FORCED_KEY", true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("server.key")).unwrap(), "FORCED_KEY");
    }

    #[test]
    fn generated_certificates_have_expected_validity_constraints_and_key_usages() {
        let (ca_cert_pem, _ca_key_pem, server_cert_pem, _server_key_pem) = generate_self_signed_certs().unwrap();

        let ca = parse_x509_pem(&ca_cert_pem);
        let leaf = parse_x509_pem(&server_cert_pem);

        assert_eq!(
            ca.not_after - ca.not_before,
            time::Duration::seconds(CA_CERT_VALIDITY.as_secs() as i64),
            "CA validity must be 10 years"
        );
        assert_eq!(
            leaf.not_after - leaf.not_before,
            time::Duration::seconds(LEAF_CERT_VALIDITY.as_secs() as i64),
            "leaf validity must be 2 years"
        );

        assert_eq!(ca.basic_constraints, Some((true, Some(0))), "CA must have pathlen:0");
        assert!(ca.key_usage_critical, "CA KeyUsage extension must be critical");
        assert!(ca.key_cert_sign, "CA must have KeyCertSign");
        assert!(ca.crl_sign, "CA must have CrlSign");

        assert_eq!(leaf.basic_constraints, Some((false, None)), "leaf must be non-CA");
        assert!(
            leaf.ext_key_usages.contains(&OID_KP_SERVER_AUTH.to_vec()),
            "leaf must have ExtendedKeyUsage::ServerAuth"
        );
        assert_eq!(leaf.issuer_cn, ca.subject_cn);
    }

    #[tokio::test]
    async fn custom_sans_are_present_and_validate_with_tonic_tls_client() {
        let sans = vec!["grpc.example.com".to_string(), "10.0.0.5".to_string()];
        let (ca_cert_pem, _ca_key_pem, server_cert_pem, server_key_pem) =
            generate_self_signed_certs_with_sans(&sans).unwrap();

        let ca = parse_x509_pem(&ca_cert_pem);
        let leaf = parse_x509_pem(&server_cert_pem);

        assert!(ca.subject_cn.contains("grpc.example.com"));
        assert!(ca.subject_cn.contains("10.0.0.5"));
        assert_eq!(leaf.sans, vec![
            SanType::DnsName("grpc.example.com".to_string()),
            SanType::IpAddress(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))),
        ]);

        let incoming = TcpIncoming::bind(([127, 0, 0, 1], 0).into()).unwrap();
        let addr = incoming.local_addr().unwrap();
        let identity = Identity::from_pem(&server_cert_pem, &server_key_pem);

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))
                .unwrap()
                .add_service(OkService)
                .serve_with_incoming(incoming)
                .await
        });

        let endpoint = Endpoint::from_shared(format!("https://{addr}"))
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(TonicCertificate::from_pem(&ca_cert_pem))
                    .domain_name("grpc.example.com"),
            )
            .unwrap();
        let _channel = endpoint
            .connect()
            .await
            .expect("TLS handshake with custom SAN succeeds");

        // Connecting with an unlisted domain name must fail hostname verification.
        let wrong_endpoint = Endpoint::from_shared(format!("https://{addr}"))
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(TonicCertificate::from_pem(&ca_cert_pem))
                    .domain_name("unlisted.example.com"),
            )
            .unwrap();
        assert!(
            wrong_endpoint.connect().await.is_err(),
            "client with non-matching domain name must fail TLS verification"
        );

        server_handle.abort();
    }

    #[tokio::test]
    async fn renew_leaf_signs_fresh_leaf_under_existing_ca_without_modifying_ca_cert() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let initial_sans = vec!["mynode.local".to_string(), "192.168.1.10".to_string()];
        let (ca_cert, ca_key, initial_leaf_cert, initial_leaf_key) =
            generate_self_signed_certs_with_sans(&initial_sans).unwrap();

        write_key_to_disk(dir.to_path_buf(), "node_ca.key", &ca_key, false).unwrap();
        write_key_to_disk(dir.to_path_buf(), "server.key", &initial_leaf_key, false).unwrap();
        write_cert_to_disk(dir.to_path_buf(), "node_ca.pem", &ca_cert).unwrap();
        write_cert_to_disk(dir.to_path_buf(), "server.pem", &initial_leaf_cert).unwrap();

        let ca_pem_before = fs::read_to_string(dir.join("node_ca.pem")).unwrap();

        let (renewed_leaf_cert, renewed_leaf_key) =
            renew_leaf_cert_from_ca_files(dir, "node_ca.pem", "node_ca.key", &initial_sans).unwrap();
        write_key_to_disk(dir.to_path_buf(), "server.key", &renewed_leaf_key, true).unwrap();
        write_cert_to_disk(dir.to_path_buf(), "server.pem", &renewed_leaf_cert).unwrap();

        let ca_pem_after = fs::read_to_string(dir.join("node_ca.pem")).unwrap();
        assert_eq!(
            ca_pem_before, ca_pem_after,
            "node_ca.pem must not be modified on leaf renewal"
        );
        assert_ne!(initial_leaf_key, renewed_leaf_key, "renewed leaf must have a fresh key");
        assert_ne!(
            initial_leaf_cert, renewed_leaf_cert,
            "renewed leaf cert must be freshly signed"
        );

        // Verify the renewed leaf validates against the original CA in a tonic TLS handshake.
        let incoming = TcpIncoming::bind(([127, 0, 0, 1], 0).into()).unwrap();
        let addr = incoming.local_addr().unwrap();
        let identity = Identity::from_pem(&renewed_leaf_cert, &renewed_leaf_key);

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))
                .unwrap()
                .add_service(OkService)
                .serve_with_incoming(incoming)
                .await
        });

        let endpoint = Endpoint::from_shared(format!("https://{addr}"))
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(TonicCertificate::from_pem(&ca_pem_after))
                    .domain_name("mynode.local"),
            )
            .unwrap();
        let _channel = endpoint
            .connect()
            .await
            .expect("renewed leaf must validate against existing CA");

        server_handle.abort();
    }

    #[test]
    fn cert_generation_error_variant_is_reachable_when_rcgen_fails() {
        // Force an rcgen error during certificate generation (RSA key generation is unsupported by ring in rcgen 0.12).
        let mut bad_ca_params = CertificateParams::new(vec!["localhost".to_string()]);
        bad_ca_params.alg = &rcgen::PKCS_RSA_SHA256;
        let leaf_params = CertificateParams::new(vec!["localhost".to_string()]);

        let err = generate_certs_from_params(bad_ca_params, leaf_params).unwrap_err();
        assert!(
            matches!(
                err,
                GrpcTlsError::CertGenerationError(rcgen::Error::KeyGenerationUnavailable)
            ),
            "expected CertGenerationError(KeyGenerationUnavailable), got: {err:?}"
        );

        // Also force an rcgen error via invalid CA key PEM in `renew_leaf_cert`.
        let (ca_cert_pem, _, _, _) = generate_self_signed_certs().unwrap();
        let pem_err = renew_leaf_cert(&ca_cert_pem, "not a valid pem", &[]).unwrap_err();
        assert!(
            matches!(pem_err, GrpcTlsError::CertGenerationError(rcgen::Error::PemError(_))),
            "expected CertGenerationError(PemError), got: {pem_err:?}"
        );
    }

    #[tokio::test]
    async fn renew_leaf_requires_ca_cert_file_and_preserves_custom_ca_subject_when_sans_change() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let initial_sans = vec!["custom-ca.example.com".to_string()];
        let (ca_cert, ca_key, initial_leaf_cert, initial_leaf_key) =
            generate_self_signed_certs_with_sans(&initial_sans).unwrap();

        write_ca_and_server_certs_to_disk(
            dir,
            "node_ca.pem",
            &ca_cert,
            "node_ca.key",
            &ca_key,
            "server.pem",
            &initial_leaf_cert,
            "server.key",
            &initial_leaf_key,
            false,
        )
        .unwrap();

        // Renew leaf without passing `--san` (defaults to `localhost`, `127.0.0.1`, `0.0.0.0`).
        // The renewed leaf must still have Issuer CommonName matching the custom CA's Subject CommonName.
        let (renewed_leaf_cert, renewed_leaf_key) =
            renew_leaf_cert_from_ca_files(dir, "node_ca.pem", "node_ca.key", &[]).unwrap();
        let ca_parsed = parse_x509_pem(&ca_cert);
        let renewed_parsed = parse_x509_pem(&renewed_leaf_cert);
        assert_eq!(
            renewed_parsed.issuer_cn, ca_parsed.subject_cn,
            "renewed leaf issuer CN must match original CA subject CN even when --san changes"
        );

        // Verify a client trusting the original CA accepts the renewed leaf when connecting to `localhost`.
        let incoming = TcpIncoming::bind(([127, 0, 0, 1], 0).into()).unwrap();
        let addr = incoming.local_addr().unwrap();
        let identity = Identity::from_pem(&renewed_leaf_cert, &renewed_leaf_key);

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))
                .unwrap()
                .add_service(OkService)
                .serve_with_incoming(incoming)
                .await
        });

        let endpoint = Endpoint::from_shared(format!("https://{addr}"))
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(TonicCertificate::from_pem(&ca_cert))
                    .domain_name("localhost"),
            )
            .unwrap();
        let _channel = endpoint
            .connect()
            .await
            .expect("client trusting original custom CA must accept leaf renewed with default SANs");

        server_handle.abort();

        // Removing `node_ca.pem` must cause `renew_leaf_cert_from_ca_files` to fail rather than guessing the CA CN.
        fs::remove_file(dir.join("node_ca.pem")).unwrap();
        let missing_ca_err = renew_leaf_cert_from_ca_files(dir, "node_ca.pem", "node_ca.key", &[]).unwrap_err();
        assert!(
            matches!(missing_ca_err, GrpcTlsError::FileReadError(_)),
            "expected FileReadError when node_ca.pem is missing, got: {missing_ca_err:?}"
        );
    }

    #[test]
    fn renew_leaf_rejects_mismatched_ca_cert_and_key() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let (ca_cert_1, _ca_key_1, _, _) = generate_self_signed_certs().unwrap();
        let (_ca_cert_2, ca_key_2, _, _) = generate_self_signed_certs().unwrap();

        let err = renew_leaf_cert_with_ca_cert(&ca_cert_1, &ca_key_2, &[]).unwrap_err();
        assert!(
            matches!(err, GrpcTlsError::CaKeyMismatch),
            "expected GrpcTlsError::CaKeyMismatch, got: {err:?}"
        );

        write_cert_to_disk(dir.to_path_buf(), "node_ca.pem", &ca_cert_1).unwrap();
        write_key_to_disk(dir.to_path_buf(), "node_ca.key", &ca_key_2, false).unwrap();

        let file_err = renew_leaf_cert_from_ca_files(dir, "node_ca.pem", "node_ca.key", &[]).unwrap_err();
        assert!(
            matches!(file_err, GrpcTlsError::CaKeyMismatch),
            "expected GrpcTlsError::CaKeyMismatch from files, got: {file_err:?}"
        );
    }

    #[test]
    fn write_cert_and_key_pair_rolls_back_key_if_cert_write_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir = temp_dir.path();

        let (_ca_cert, _ca_key, initial_cert, initial_key) = generate_self_signed_certs().unwrap();
        write_cert_and_key_pair_to_disk(dir, "server.pem", &initial_cert, "server.key", &initial_key, false).unwrap();

        let (_, _, new_cert, new_key) = generate_self_signed_certs().unwrap();

        // Make `server.pem` read-only (readable, not writable) so replacing `server.pem` fails after `server.key` is
        // written.
        let cert_path = dir.join("server.pem");
        let mut perms = fs::metadata(&cert_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&cert_path, perms.clone()).unwrap();

        let res = write_cert_and_key_pair_to_disk(dir, "server.pem", &new_cert, "server.key", &new_key, true);
        assert!(res.is_err(), "writing to a read-only server.pem must fail");

        // Restore write permission so temp_dir cleanup succeeds on Windows.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cert_path, fs::Permissions::from_mode(0o644)).unwrap();
        }
        #[cfg(not(unix))]
        {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            fs::set_permissions(&cert_path, perms).unwrap();
        }

        assert_eq!(
            fs::read_to_string(dir.join("server.key")).unwrap(),
            initial_key,
            "original server.key must be preserved when server.pem write fails"
        );
        assert_eq!(
            fs::read_to_string(dir.join("server.pem")).unwrap(),
            initial_cert,
            "original server.pem must be preserved when server.pem write fails"
        );
    }
}
