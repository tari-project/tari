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

use rcgen::{generate_simple_self_signed, Certificate, CertificateParams, DnType, IsCa::Ca};

use crate::tls::error::GrpcTlsError;

pub fn generate_self_signed_certs() -> Result<(String, String, String), GrpcTlsError> {
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string(), "0.0.0.0".to_string()];
    let mut params = CertificateParams::new(subject_alt_names.clone());
    params.distinguished_name.push(DnType::CommonName, "127.0.0.1");
    params.is_ca = Ca(rcgen::BasicConstraints::Unconstrained);
    let ca = Certificate::from_params(params).unwrap();
    let cacert = ca.serialize_pem().unwrap();

    let server_cert = generate_simple_self_signed(subject_alt_names).unwrap();

    Ok((
        cacert,
        server_cert.serialize_pem_with_signer(&ca).unwrap(),
        server_cert.serialize_private_key_pem(),
    ))
}

pub fn write_cert_to_disk(dir: PathBuf, filename: &str, data: &String) -> Result<(), GrpcTlsError> {
    let path = dir.join(Path::new(filename));
    let mut file = File::create(&path)?;
    file.write_all(data.as_ref())?;

    println!("{:?} written to disk.", path);

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
