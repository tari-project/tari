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
    time::{SystemTime, UNIX_EPOCH},
};

use rcgen::{
    BasicConstraints,
    CertificateParams,
    DnType,
    ExtendedKeyUsagePurpose,
    IsCa,
    Issuer,
    KeyPair,
    KeyUsagePurpose,
    PublicKeyData,
    date_time_ymd,
};
use time::OffsetDateTime;
use yasna::{
    models::ObjectIdentifier,
    tags::{TAG_IA5STRING, TAG_PRINTABLESTRING, TAG_UTCTIME, TAG_UTF8STRING},
};

use crate::tls::error::GrpcTlsError;

/// Default Subject Alternative Names included when `--san` is not supplied.
pub const DEFAULT_TLS_SANS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0"];

/// Default CommonName used for generated self-signed CA certificates.
pub const DEFAULT_CA_COMMON_NAME: &str = "Minotari gRPC CA";

/// Advance warning window (in days) before a finite-validity leaf certificate expires.
pub const CERT_EXPIRY_WARNING_DAYS: i64 = 30;

const OID_COMMON_NAME: &[u64] = &[2, 5, 4, 3];

/// Returns the default `not_after` sentinel (`9999-12-31 00:00:00 UTC`) used when no finite validity period is set.
pub fn no_expiration_date_time() -> OffsetDateTime {
    date_time_ymd(9999, 12, 31)
}

pub(crate) fn current_utc_second() -> OffsetDateTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs_i64 = i64::try_from(secs).unwrap_or(i64::MAX);
    date_time_ymd(1970, 1, 1)
        .checked_add(time::Duration::seconds(secs_i64))
        .unwrap_or_else(no_expiration_date_time)
}

fn compute_not_after(not_before: OffsetDateTime, validity_days: Option<u64>) -> Result<OffsetDateTime, GrpcTlsError> {
    let Some(days) = validity_days else {
        return Ok(no_expiration_date_time());
    };
    if days == 0 {
        return Err(GrpcTlsError::InvalidValidityDays(0));
    }
    let days_i64 = i64::try_from(days).map_err(|_| GrpcTlsError::InvalidValidityDays(days))?;
    let seconds = days_i64
        .checked_mul(86_400)
        .ok_or(GrpcTlsError::InvalidValidityDays(days))?;
    let duration = time::Duration::seconds(seconds);
    let candidate = not_before
        .checked_add(duration)
        .ok_or(GrpcTlsError::InvalidValidityDays(days))?;
    let max_not_after = no_expiration_date_time();
    Ok(if candidate > max_not_after {
        max_not_after
    } else {
        candidate
    })
}

fn resolve_sans(subject_alt_names: &[String]) -> Vec<String> {
    if subject_alt_names.is_empty() {
        DEFAULT_TLS_SANS.iter().map(|s| (*s).to_string()).collect()
    } else {
        subject_alt_names.to_vec()
    }
}

fn build_ca_params(common_name: &str, not_before: OffsetDateTime, not_after: OffsetDateTime) -> CertificateParams {
    let mut params = CertificateParams::default();
    params.distinguished_name.push(DnType::CommonName, common_name);
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = not_before;
    params.not_after = not_after;
    params
}

fn build_leaf_params(
    subject_alt_names: &[String],
    not_before: OffsetDateTime,
    not_after: OffsetDateTime,
) -> Result<CertificateParams, GrpcTlsError> {
    let mut params = CertificateParams::new(subject_alt_names.to_vec())?;
    params
        .distinguished_name
        .push(DnType::CommonName, subject_alt_names.join(", "));
    params.is_ca = IsCa::ExplicitNoCa;
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = not_before;
    params.not_after = not_after;
    Ok(params)
}

pub(crate) fn generate_certs_from_params(
    ca_params: CertificateParams,
    leaf_params: CertificateParams,
) -> Result<(String, String, String, String), GrpcTlsError> {
    let ca_key_pair = KeyPair::generate()?;
    let ca = ca_params.self_signed(&ca_key_pair)?;
    let ca_issuer = Issuer::from_params(&ca_params, &ca_key_pair);
    let cacert = ca.pem();
    let ca_key = ca_key_pair.serialize_pem();

    let server_key_pair = KeyPair::generate()?;
    let server_cert = leaf_params.signed_by(&server_key_pair, &ca_issuer)?;
    let cert = server_cert.pem();
    let private_key = server_key_pair.serialize_pem();

    Ok((cacert, ca_key, cert, private_key))
}

/// Generates a constrained self-signed CA certificate (`pathlen:0`, `KeyCertSign` + `CrlSign`, default no-expiry
/// `9999-12-31`) and a server leaf certificate (`ServerAuth`, default no-expiry `9999-12-31`) using the default SAN set
/// (`localhost`, `127.0.0.1`, `0.0.0.0`).
///
/// Returns `(ca_cert_pem, ca_key_pem, server_cert_pem, server_key_pem)`.
pub fn generate_self_signed_certs() -> Result<(String, String, String, String), GrpcTlsError> {
    generate_self_signed_certs_with_sans_and_validity(&[], None, None)
}

/// Generates a constrained self-signed CA certificate and a server leaf certificate for the provided Subject
/// Alternative Names (falling back to [`DEFAULT_TLS_SANS`] when `subject_alt_names` is empty), defaulting to no
/// expiration (`9999-12-31`).
///
/// Returns `(ca_cert_pem, ca_key_pem, server_cert_pem, server_key_pem)`.
pub fn generate_self_signed_certs_with_sans(
    subject_alt_names: &[String],
) -> Result<(String, String, String, String), GrpcTlsError> {
    generate_self_signed_certs_with_sans_and_validity(subject_alt_names, None, None)
}

/// Generates a constrained self-signed CA certificate and a server leaf certificate for the provided Subject
/// Alternative Names and optional validity periods in days.
///
/// When `leaf_validity_days` and `ca_validity_days` are both `None`, both certificates default to no expiration
/// (`9999-12-31`). The CA validity is always clamped to at least the leaf's validity so the CA never expires before the
/// leaf it signs.
///
/// Returns `(ca_cert_pem, ca_key_pem, server_cert_pem, server_key_pem)`.
pub fn generate_self_signed_certs_with_sans_and_validity(
    subject_alt_names: &[String],
    leaf_validity_days: Option<u64>,
    ca_validity_days: Option<u64>,
) -> Result<(String, String, String, String), GrpcTlsError> {
    let sans = resolve_sans(subject_alt_names);
    let not_before = current_utc_second();
    let (leaf_not_after, ca_not_after) = match (leaf_validity_days, ca_validity_days) {
        (None, Some(ca_days)) => {
            let ca_na = compute_not_after(not_before, Some(ca_days))?;
            (ca_na, ca_na)
        },
        (leaf_days, ca_days) => {
            let leaf_na = compute_not_after(not_before, leaf_days)?;
            let raw_ca_na = compute_not_after(not_before, ca_days)?;
            let ca_na = if raw_ca_na < leaf_na { leaf_na } else { raw_ca_na };
            (leaf_na, ca_na)
        },
    };
    let ca_params = build_ca_params(DEFAULT_CA_COMMON_NAME, not_before, ca_not_after);
    let leaf_params = build_leaf_params(&sans, not_before, leaf_not_after)?;
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
    renew_leaf_cert_with_ca_cert_and_validity(ca_cert_pem, ca_key_pem, subject_alt_names, None)
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
    renew_leaf_cert_with_ca_cert_and_validity(ca_cert_pem, ca_key_pem, subject_alt_names, None)
}

/// Regenerates a fresh leaf certificate and private key signed by the existing CA certificate and private key with an
/// optional validity period in days.
///
/// The renewed leaf's `not_after` defaults to no expiration (`9999-12-31`) when `validity_days` is `None`, and is
/// always capped at the existing CA certificate's `not_after` so the leaf never outlives its signer.
pub fn renew_leaf_cert_with_ca_cert_and_validity(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    subject_alt_names: &[String],
    validity_days: Option<u64>,
) -> Result<(String, String), GrpcTlsError> {
    let sans = resolve_sans(subject_alt_names);
    let ca_key_pair = KeyPair::from_pem(ca_key_pem)?;
    let (ca_cn, ca_spki_der, ca_not_after) = extract_subject_cn_spki_and_not_after_from_pem(ca_cert_pem)?;
    if ca_spki_der != ca_key_pair.subject_public_key_info() {
        return Err(GrpcTlsError::CaKeyMismatch);
    }
    let not_before = current_utc_second();
    if ca_not_after <= not_before {
        return Err(GrpcTlsError::CaExpired(ca_not_after.to_string()));
    }
    let requested_leaf_not_after = compute_not_after(not_before, validity_days)?;
    let leaf_not_after = if requested_leaf_not_after > ca_not_after {
        ca_not_after
    } else {
        requested_leaf_not_after
    };
    renew_leaf_cert_with_ca_key_pair(ca_key_pair, &ca_cn, &sans, not_before, ca_not_after, leaf_not_after)
}

fn renew_leaf_cert_with_ca_key_pair(
    ca_key_pair: KeyPair,
    ca_common_name: &str,
    sans: &[String],
    not_before: OffsetDateTime,
    ca_not_after: OffsetDateTime,
    leaf_not_after: OffsetDateTime,
) -> Result<(String, String), GrpcTlsError> {
    let ca_params = build_ca_params(ca_common_name, not_before, ca_not_after);
    let ca_issuer = Issuer::from_params(&ca_params, &ca_key_pair);

    let leaf_params = build_leaf_params(sans, not_before, leaf_not_after)?;
    let server_key_pair = KeyPair::generate()?;
    let server_cert = leaf_params.signed_by(&server_key_pair, &ca_issuer)?;
    let cert_pem = server_cert.pem();
    let key_pem = server_key_pair.serialize_pem();

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
    renew_leaf_cert_from_ca_files_with_validity(dir, ca_cert_filename, ca_key_filename, subject_alt_names, None)
}

/// Reads the persisted CA certificate and CA private key from `dir`, verifies that they form a matching key pair, and
/// signs a fresh leaf certificate and private key with an optional validity period in days.
pub fn renew_leaf_cert_from_ca_files_with_validity(
    dir: &Path,
    ca_cert_filename: &str,
    ca_key_filename: &str,
    subject_alt_names: &[String],
    validity_days: Option<u64>,
) -> Result<(String, String), GrpcTlsError> {
    let ca_key_path = dir.join(ca_key_filename);
    let ca_key_pem = fs::read_to_string(&ca_key_path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            GrpcTlsError::FileReadError(format!(
                "Could not load the file `{ca_key_path:?}`: {e}. Certificates generated before `{ca_key_filename}` \
                 was persisted cannot renew the leaf in place; run `create-tls-certs --force` once to regenerate the \
                 CA and server bundle, then redistribute `{ca_cert_filename}` to clients."
            ))
        } else {
            GrpcTlsError::FileReadError(format!("Could not load the file `{ca_key_path:?}`: {e}"))
        }
    })?;

    let ca_cert_path = dir.join(ca_cert_filename);
    let ca_cert_pem = fs::read_to_string(&ca_cert_path)
        .map_err(|e| GrpcTlsError::FileReadError(format!("Could not load the file `{ca_cert_path:?}`: {e}")))?;

    renew_leaf_cert_with_ca_cert_and_validity(&ca_cert_pem, &ca_key_pem, subject_alt_names, validity_days)
}

/// Extracts the `not_after` timestamp from a PEM-encoded X.509 certificate.
pub fn extract_cert_not_after_from_pem(cert_pem: &str) -> Result<OffsetDateTime, GrpcTlsError> {
    let (_cn, _spki, not_after) = extract_subject_cn_spki_and_not_after_from_pem(cert_pem)?;
    Ok(not_after)
}

fn read_asn1_time(reader: yasna::BERReader<'_, '_>) -> Result<OffsetDateTime, yasna::ASN1Error> {
    if reader.lookahead_tag()? == TAG_UTCTIME {
        Ok(*reader.read_utctime()?.datetime())
    } else {
        Ok(*reader.read_generalized_time()?.datetime())
    }
}

fn extract_subject_cn_spki_and_not_after_from_pem(
    cert_pem: &str,
) -> Result<(String, Vec<u8>, OffsetDateTime), GrpcTlsError> {
    let parsed_pem = pem::parse(cert_pem).map_err(|_| rcgen::Error::CouldNotParseCertificate)?;
    if parsed_pem.tag() != "CERTIFICATE" {
        return Err(rcgen::Error::CouldNotParseCertificate.into());
    }
    let cn_oid = ObjectIdentifier::from_slice(OID_COMMON_NAME);

    let (subject_cn, spki_der, not_after) = yasna::parse_der(parsed_pem.contents(), |reader| {
        reader.read_sequence(|reader| {
            let tbs = reader.next().read_sequence(|reader| {
                // version [0] EXPLICIT
                drop(reader.next().read_der()?);
                // serialNumber
                drop(reader.next().read_der()?);
                // signature AlgorithmIdentifier
                drop(reader.next().read_der()?);
                // issuer Name
                drop(reader.next().read_der()?);
                // validity
                let (_not_before, not_after) = reader.next().read_sequence(|reader| {
                    let nb = read_asn1_time(reader.next())?;
                    let na = read_asn1_time(reader.next())?;
                    Ok((nb, na))
                })?;
                // subject Name
                let cn = read_common_name(reader.next(), &cn_oid)?;
                // subjectPublicKeyInfo
                let spki_der = reader.next().read_der()?;
                // optional extensions
                drop(reader.read_optional(|r| r.read_der())?);
                Ok((cn, spki_der, not_after))
            })?;
            // signatureAlgorithm
            drop(reader.next().read_der()?);
            // signatureValue
            drop(reader.next().read_der()?);
            Ok(tbs)
        })
    })
    .map_err(|_| GrpcTlsError::from(rcgen::Error::CouldNotParseCertificate))?;

    if subject_cn.is_empty() {
        return Err(rcgen::Error::CouldNotParseCertificate.into());
    }

    Ok((subject_cn, spki_der, not_after))
}

fn read_common_name(reader: yasna::BERReader<'_, '_>, cn_oid: &ObjectIdentifier) -> Result<String, yasna::ASN1Error> {
    let rdns = reader.collect_sequence_of(|reader| {
        reader.collect_set_of(|reader| {
            reader.read_sequence(|reader| {
                let oid = reader.next().read_oid()?;
                let val_reader = reader.next();
                let tag = val_reader.lookahead_tag()?;
                let val = if tag == TAG_UTF8STRING {
                    val_reader.read_utf8string()?
                } else if tag == TAG_PRINTABLESTRING || tag == TAG_IA5STRING {
                    // PrintableString (19) or IA5String (22)
                    val_reader.read_tagged_implicit(tag, |r| {
                        String::from_utf8(r.read_bytes()?)
                            .map_err(|_| yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid))
                    })?
                } else {
                    return Err(yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid));
                };
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
    data: &'a [u8],
    is_key: bool,
}

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_staged_temp_file(
    dir: &Path,
    entry: StagedTlsFile<'_>,
    index: usize,
    custom_perms: Option<fs::Permissions>,
) -> Result<PathBuf, io::Error> {
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
        let perms =
            custom_perms.unwrap_or_else(|| fs::Permissions::from_mode(if entry.is_key { 0o600 } else { 0o644 }));
        file.set_permissions(perms)?;
    }
    #[cfg(not(unix))]
    if let Some(perms) = custom_perms {
        file.set_permissions(perms)?;
    }
    if let Err(err) = file.write_all(entry.data).and_then(|_| file.sync_all()) {
        drop(fs::remove_file(&temp_path));
        return Err(err);
    }
    Ok(temp_path)
}

type StagedBackup = (PathBuf, String, Option<(Vec<u8>, fs::Permissions, bool)>);

fn restore_backups(dir: &Path, committed_paths: &[PathBuf], backups: &[StagedBackup]) {
    for (idx, path) in committed_paths.iter().enumerate() {
        if let Some((_, filename, backup_opt)) = backups.iter().find(|(p, _, _)| p == path) {
            match backup_opt {
                Some((old_bytes, old_perms, is_key)) => {
                    let staged = StagedTlsFile {
                        filename,
                        data: old_bytes,
                        is_key: *is_key,
                    };
                    if let Ok(temp_path) = create_staged_temp_file(dir, staged, idx, Some(old_perms.clone())) &&
                        fs::rename(&temp_path, path).is_err()
                    {
                        drop(fs::remove_file(&temp_path));
                    }
                },
                None => {
                    drop(fs::remove_file(path));
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

    // 2. Stage all new files into temporary files in `dir` with final permissions and `sync_all()`.
    let mut staged_paths = Vec::with_capacity(entries.len());
    for (idx, entry) in entries.iter().copied().enumerate() {
        match create_staged_temp_file(dir, entry, idx, None) {
            Ok(temp_path) => staged_paths.push(temp_path),
            Err(err) => {
                for p in &staged_paths {
                    drop(fs::remove_file(p));
                }
                return Err(GrpcTlsError::IoError(err));
            },
        }
    }

    // 3. Snapshot any existing target files so we can roll back if any subsequent rename fails.
    let mut backups: Vec<StagedBackup> = Vec::with_capacity(entries.len());
    for entry in entries {
        let target_path = dir.join(entry.filename);
        if fs::symlink_metadata(&target_path).is_ok() {
            let snapshot = fs::metadata(&target_path).and_then(|meta| {
                let bytes = fs::read(&target_path)?;
                Ok((bytes, meta.permissions(), entry.is_key))
            });
            match snapshot {
                Ok(data) => backups.push((target_path, entry.filename.to_string(), Some(data))),
                Err(err) => {
                    for p in &staged_paths {
                        drop(fs::remove_file(p));
                    }
                    return Err(GrpcTlsError::IoError(err));
                },
            }
        } else {
            backups.push((target_path, entry.filename.to_string(), None));
        }
    }

    // 4. Atomically rename each staged file over its target path; if any rename fails, restore all previously committed
    //    files.
    let mut committed_paths: Vec<PathBuf> = Vec::with_capacity(entries.len());
    for (entry, staged_path) in entries.iter().zip(staged_paths.iter()) {
        let target_path = dir.join(entry.filename);
        let rename_res = match fs::metadata(&target_path) {
            Ok(meta) if meta.permissions().readonly() => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Target file `{target_path:?}` is read-only"),
            )),
            _ => fs::rename(staged_path, &target_path),
        };
        match rename_res {
            Ok(()) => committed_paths.push(target_path),
            Err(err) => {
                for remaining in &staged_paths {
                    drop(fs::remove_file(remaining));
                }
                restore_backups(dir, &committed_paths, &backups);
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
            data: key_pem.as_bytes(),
            is_key: true,
        },
        StagedTlsFile {
            filename: cert_filename,
            data: cert_pem.as_bytes(),
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
            data: server_key_pem.as_bytes(),
            is_key: true,
        },
        StagedTlsFile {
            filename: ca_key_filename,
            data: ca_key_pem.as_bytes(),
            is_key: true,
        },
        StagedTlsFile {
            filename: ca_cert_filename,
            data: ca_cert_pem.as_bytes(),
            is_key: false,
        },
        StagedTlsFile {
            filename: server_cert_filename,
            data: server_cert_pem.as_bytes(),
            is_key: false,
        },
    ];
    write_staged_tls_files(dir, &entries, force)
}

/// Writes a public TLS certificate artifact (`.pem`) to `dir/filename`.
///
/// On Unix, the file mode is explicitly set to `0644`.
pub fn write_cert_to_disk(dir: PathBuf, filename: &str, data: &str) -> Result<(), GrpcTlsError> {
    let entries = [StagedTlsFile {
        filename,
        data: data.as_bytes(),
        is_key: false,
    }];
    write_staged_tls_files(&dir, &entries, true)
}

/// Writes a private TLS key artifact (`.key`) to `dir/filename`.
///
/// Refuses to overwrite an existing key file unless `force` is `true`.
/// On Unix, the file mode is explicitly set to `0600`. On Windows, restricting the configuration directory ACL is the
/// operator's responsibility.
pub fn write_key_to_disk(dir: PathBuf, filename: &str, data: &str, force: bool) -> Result<(), GrpcTlsError> {
    let entries = [StagedTlsFile {
        filename,
        data: data.as_bytes(),
        is_key: true,
    }];
    write_staged_tls_files(&dir, &entries, force)
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
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::else_if_without_else,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    let_underscore_drop
)]
mod tests {
    use std::{
        convert::Infallible,
        io,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        task::{Context, Poll},
    };

    use rcgen::SanType;
    use tokio::{net::TcpListener, sync::oneshot};
    use tonic::{
        Request,
        Response,
        Status,
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
    use yasna::Tag;

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
                    drop(reader.next().read_der()?);
                    // signature
                    drop(reader.next().read_der()?);
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
                    drop(reader.next().read_der()?);
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
                                        Ok(SanType::DnsName(
                                            dns.try_into()
                                                .map_err(|_| yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid))?,
                                        ))
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
                        } else {
                            // Ignore other X.509 extensions.
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
                drop(reader.next().read_der()?);
                // signatureValue
                drop(reader.next().read_der()?);
                Ok(tbs)
            })
        })
        .expect("valid X.509 DER")
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
            ca.not_after,
            no_expiration_date_time(),
            "CA default not_after must be 9999-12-31 no-expiration"
        );
        assert_eq!(
            leaf.not_after,
            no_expiration_date_time(),
            "leaf default not_after must be 9999-12-31 no-expiration"
        );
        assert_eq!(ca.subject_cn, DEFAULT_CA_COMMON_NAME);

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

        // Custom finite validity (--validity-days 365, --ca-validity-days 90) clamps CA validity to at least the leaf.
        let (finite_ca_pem, _, finite_leaf_pem, _) =
            generate_self_signed_certs_with_sans_and_validity(&[], Some(365), Some(90)).unwrap();
        let finite_ca = parse_x509_pem(&finite_ca_pem);
        let finite_leaf = parse_x509_pem(&finite_leaf_pem);
        assert_eq!(
            finite_leaf.not_after - finite_leaf.not_before,
            time::Duration::days(365)
        );
        assert_eq!(
            finite_ca.not_after - finite_ca.not_before,
            time::Duration::days(365),
            "CA validity must be clamped to at least the leaf validity"
        );

        // Zero validity days must be rejected.
        let zero_err = generate_self_signed_certs_with_sans_and_validity(&[], Some(0), None).unwrap_err();
        assert!(matches!(zero_err, GrpcTlsError::InvalidValidityDays(0)));
    }

    #[test]
    fn renew_leaf_caps_validity_at_ca_not_after_and_reports_hint_when_ca_key_missing() {
        let (ca_cert_pem, ca_key_pem, _, _) =
            generate_self_signed_certs_with_sans_and_validity(&[], Some(30), Some(30)).unwrap();
        let ca = parse_x509_pem(&ca_cert_pem);

        // Renewing without `--validity-days` (default no-expiry) or with `--validity-days 90` must cap at CA not_after.
        let (renewed_default_pem, _) =
            renew_leaf_cert_with_ca_cert_and_validity(&ca_cert_pem, &ca_key_pem, &[], None).unwrap();
        let renewed_default = parse_x509_pem(&renewed_default_pem);
        assert_eq!(renewed_default.not_after, ca.not_after);

        let (renewed_90_pem, _) =
            renew_leaf_cert_with_ca_cert_and_validity(&ca_cert_pem, &ca_key_pem, &[], Some(90)).unwrap();
        let renewed_90 = parse_x509_pem(&renewed_90_pem);
        assert_eq!(renewed_90.not_after, ca.not_after);

        // Renewing with `--validity-days 10` (< CA 30 days) uses the shorter 10-day validity.
        let (renewed_10_pem, _) =
            renew_leaf_cert_with_ca_cert_and_validity(&ca_cert_pem, &ca_key_pem, &[], Some(10)).unwrap();
        let renewed_10 = parse_x509_pem(&renewed_10_pem);
        assert_eq!(renewed_10.not_after - renewed_10.not_before, time::Duration::days(10));

        // When `<app>_ca.key` is missing on disk, the error message guides operators to `create-tls-certs --force`.
        let temp_dir = tempfile::tempdir().unwrap();
        write_cert_to_disk(temp_dir.path().to_path_buf(), "node_ca.pem", &ca_cert_pem).unwrap();
        let missing_key_err =
            renew_leaf_cert_from_ca_files(temp_dir.path(), "node_ca.pem", "node_ca.key", &[]).unwrap_err();
        match missing_key_err {
            GrpcTlsError::FileReadError(msg) => {
                assert!(
                    msg.contains("create-tls-certs --force"),
                    "expected hint mentioning `create-tls-certs --force`, got: {msg}"
                );
            },
            other => panic!("expected FileReadError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn generates_valid_ca_and_server_tls_pem_bundle() {
        let (ca_pem, _ca_key_pem, server_cert_pem, server_key_pem) = generate_self_signed_certs().unwrap();
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
                    .ca_certificate(TonicCertificate::from_pem(ca_pem.as_bytes()))
                    .domain_name("localhost"),
            )
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = ShaP2PoolClient::new(trusted_channel);
        client.get_tip_info(GetTipInfoRequest {}).await.unwrap();

        // 2. Client trusting an unrelated CA rejects the server certificate during the TLS handshake.
        let (unrelated_ca_pem, _, _, _) = generate_self_signed_certs().unwrap();
        let untrusted_result = Endpoint::from_shared(url)
            .unwrap()
            .tls_config(
                ClientTlsConfig::new()
                    .ca_certificate(TonicCertificate::from_pem(unrelated_ca_pem.as_bytes()))
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

    #[tokio::test]
    async fn custom_sans_are_present_and_validate_with_tonic_tls_client() {
        let sans = vec!["grpc.example.com".to_string(), "10.0.0.5".to_string()];
        let (ca_cert_pem, _ca_key_pem, server_cert_pem, server_key_pem) =
            generate_self_signed_certs_with_sans(&sans).unwrap();

        let ca = parse_x509_pem(&ca_cert_pem);
        let leaf = parse_x509_pem(&server_cert_pem);

        assert_eq!(ca.subject_cn, DEFAULT_CA_COMMON_NAME);
        assert!(leaf.subject_cn.contains("grpc.example.com"));
        assert!(leaf.subject_cn.contains("10.0.0.5"));
        assert_eq!(leaf.sans, vec![
            SanType::DnsName("grpc.example.com".try_into().unwrap()),
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
        drop(
            endpoint
                .connect()
                .await
                .expect("TLS handshake with custom SAN succeeds"),
        );

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
        drop(
            endpoint
                .connect()
                .await
                .expect("renewed leaf must validate against existing CA"),
        );

        server_handle.abort();
    }

    #[test]
    fn cert_generation_error_variant_is_reachable_when_rcgen_fails() {
        // Force an rcgen error during certificate parameter construction (non-ASCII SAN is rejected by Ia5String).
        let err = generate_self_signed_certs_with_sans(&["invalid\u{00e9}.example.com".to_string()]).unwrap_err();
        assert!(
            matches!(err, GrpcTlsError::CertGenerationError(_)),
            "expected CertGenerationError, got: {err:?}"
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
        let not_before = current_utc_second();
        let ca_params = build_ca_params("Custom Operator CA", not_before, no_expiration_date_time());
        let leaf_params = build_leaf_params(&initial_sans, not_before, no_expiration_date_time()).unwrap();
        let (ca_cert, ca_key, initial_leaf_cert, initial_leaf_key) =
            generate_certs_from_params(ca_params, leaf_params).unwrap();

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
        assert_eq!(ca_parsed.subject_cn, "Custom Operator CA");
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
        drop(
            endpoint
                .connect()
                .await
                .expect("client trusting original custom CA must accept leaf renewed with default SANs"),
        );

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
