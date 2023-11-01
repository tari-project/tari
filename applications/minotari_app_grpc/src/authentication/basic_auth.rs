//  Copyright 2022. The Tari Project
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

use std::{borrow::Cow, cmp::max, ops::Deref, string::FromUtf8Error};

use argon2::{password_hash::Encoding, Argon2, PasswordHash, PasswordVerifier};
use rand::RngCore;
use tari_utilities::SafePassword;
use tonic::metadata::{errors::InvalidMetadataValue, Ascii, MetadataValue};
use zeroize::{Zeroize, Zeroizing};

const MAX_USERNAME_LEN: usize = 256;

/// Implements [RFC 2617](https://www.ietf.org/rfc/rfc2617.txt#:~:text=The%20%22basic%22%20authentication%20scheme%20is,other%20realms%20on%20that%20server.)
/// Represents the username and password contained within a Authenticate header.
#[derive(Debug)]
pub struct BasicAuthCredentials {
    pub user_name: String,
    pub password: SafePassword,
}

impl BasicAuthCredentials {
    pub fn new(user_name: String, password: SafePassword) -> Result<Self, BasicAuthError> {
        if user_name.as_bytes().is_empty() || user_name.as_bytes().len() > MAX_USERNAME_LEN {
            return Err(BasicAuthError::InvalidUsername);
        }
        Ok(Self { user_name, password })
    }

    /// Creates a `Credentials` instance from a base64 `String`
    /// which must encode user credentials as `username:password`
    pub fn decode(auth_header_value: &str) -> Result<Self, BasicAuthError> {
        let decoded = base64::decode(auth_header_value)?;
        let as_utf8 = Zeroizing::new(String::from_utf8(decoded)?);

        if let Some((user_name, password)) = as_utf8.split_once(':') {
            let credentials = Self::new(user_name.into(), password.to_string().into())?;
            return Ok(credentials);
        }

        Err(BasicAuthError::InvalidAuthorizationHeader)
    }

    /// Creates a `Credentials` instance from an HTTP Authorization header
    /// which schema is a valid `Basic` HTTP Authorization Schema.
    pub fn from_header(auth_header: &str) -> Result<BasicAuthCredentials, BasicAuthError> {
        // check if its a valid basic auth header
        let (auth_type, encoded_credentials) = auth_header
            .split_once(' ')
            .ok_or(BasicAuthError::InvalidAuthorizationHeader)?;

        if encoded_credentials.contains(' ') {
            // Invalid authorization token received
            return Err(BasicAuthError::InvalidAuthorizationHeader);
        }

        // Check the provided authorization header
        // to be a "Basic" authorization header
        if auth_type.to_lowercase() != "basic" {
            return Err(BasicAuthError::InvalidScheme(auth_type.to_string()));
        }

        let credentials = BasicAuthCredentials::decode(encoded_credentials)?;
        Ok(credentials)
    }

    // This function provides an approximate constant time comparison of the given username with the registered
    // username, and is not meant to be an optimally performant comparison. This function also returns a boolean
    // instead of a Result as the 'Ok' and 'Err' variants of the Result would have different execution times,
    // It is also acceptable for the function to be more or less performant on different platforms, as long as the
    // variance is within acceptable limits.
    fn constant_time_compare_username(&self, username: &str) -> bool {
        const BUFFER_MULTIPLIER: usize = 16;
        const OPERATE_LEN: usize = MAX_USERNAME_LEN * BUFFER_MULTIPLIER;

        let a_bytes = self.user_name.as_bytes();
        let b_bytes = username.as_bytes();

        // We do not care if this check returns immediately, as the the maximum allowed value is sufficiently long
        // and it will not leak any information about the correctness of the supplied username other than it is too
        // long.
        if b_bytes.is_empty() || b_bytes.len() > MAX_USERNAME_LEN {
            return false;
        }

        // Comparison bytes for both usernames are initialized to a large array of equal random bytes.
        let mut a_compare_bytes = [0u8; OPERATE_LEN];
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut a_compare_bytes);
        let mut b_compare_bytes = a_compare_bytes;

        // Both username's bytes are interleaved with the respective comparison byte arrays.
        let interleave_factor = max(BUFFER_MULTIPLIER / 3, 1);
        let a_repeat = OPERATE_LEN / (a_bytes.len() * interleave_factor);
        for (i, (j, byte)) in (0..a_repeat).zip(a_bytes.iter()).enumerate() {
            a_compare_bytes[i * a_bytes.len() * interleave_factor + j * interleave_factor] = *byte;
        }
        let b_repeat = OPERATE_LEN / (b_bytes.len() * interleave_factor);
        for (i, (j, byte)) in (0..b_repeat).zip(b_bytes.iter()).enumerate() {
            b_compare_bytes[i * b_bytes.len() * interleave_factor + j * interleave_factor] = *byte;
        }

        // Perform a constant time bitwise comparison of the two byte arrays and accumulate the result.
        let mut result = 0;
        for (x, y) in a_compare_bytes.iter().zip(b_compare_bytes.iter()) {
            result |= x ^ y;
        }

        result == 0
    }

    pub fn validate(&self, username: &str, password: &[u8]) -> Result<(), BasicAuthError> {
        if !self.constant_time_compare_username(username) {
            return Err(BasicAuthError::InvalidUsername);
        }
        // These bytes can leak if the password is not utf-8, but since argon encoding is utf-8 the given
        // password must be incorrect if conversion to utf-8 fails.
        let bytes = self.password.reveal().to_vec();
        let str_password = Zeroizing::new(String::from_utf8(bytes)?);
        let header_password = PasswordHash::parse(&str_password, Encoding::B64)?;
        Argon2::default().verify_password(password, &header_password)?;
        Ok(())
    }

    pub fn generate_header(username: &str, password: &[u8]) -> Result<MetadataValue<Ascii>, BasicAuthError> {
        let password_str = String::from_utf8_lossy(password);
        let token_str = Zeroizing::new(format!("{}:{}", username, password_str));
        let mut token = base64::encode(token_str.deref());
        let header = format!("Basic {}", token);
        token.zeroize();
        match password_str {
            Cow::Borrowed(_) => {},
            Cow::Owned(mut owned) => owned.zeroize(),
        }
        let header = header.parse()?;
        Ok(header)
    }
}

/// Authorization Header Error
#[derive(Debug, thiserror::Error)]
pub enum BasicAuthError {
    #[error("Invalid username")]
    InvalidUsername,
    #[error("The HTTP Authorization header value is invalid")]
    InvalidAuthorizationHeader,
    #[error("The HTTP Authorization header contains an invalid scheme {0} but only `Basic` is supported")]
    InvalidScheme(String),
    #[error("The value expected as a base64 encoded `String` is not encoded correctly: {0}")]
    InvalidBase64Value(#[from] base64::DecodeError),
    #[error("The provided binary is not a valid UTF-8 character: {0}")]
    InvalidUtf8Value(#[from] FromUtf8Error),
    #[error("Invalid password: {0}")]
    InvalidPassword(#[from] argon2::password_hash::Error),
    #[error("Invalid header value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    mod from_header {
        use super::*;

        #[test]
        fn it_decodes_from_well_formed_header() {
            let credentials = BasicAuthCredentials::from_header("Basic YWRtaW46c2VjcmV0").unwrap();
            assert_eq!(credentials.user_name, "admin");
            assert_eq!(credentials.password.reveal(), b"secret");
        }

        #[test]
        fn it_rejects_header_without_basic_scheme() {
            let err = BasicAuthCredentials::from_header(" YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "");
            } else {
                panic!("Unexpected error: {:?}", err);
            };
            let err = BasicAuthCredentials::from_header("Cookie YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "Cookie");
            } else {
                panic!("Unexpected error: {:?}", err);
            };
        }
    }

    mod validate {
        use std::{cmp::min, thread::sleep};

        use tari_utilities::{hex::Hex, ByteArray};

        use super::*;
        use crate::authentication::salted_password::create_salted_hashed_password;

        #[test]
        fn it_validates_for_matching_credentials() {
            let hashed = create_salted_hashed_password(b"secret").unwrap();
            let credentials = BasicAuthCredentials::new("admin".to_string(), hashed.to_string().into()).unwrap();
            credentials.validate("admin", b"secret").unwrap();
        }

        #[test]
        fn it_rejects_for_mismatching_credentials() {
            let credentials = BasicAuthCredentials::new("admin".to_string(), "bruteforce".to_string().into()).unwrap();
            let err = credentials.validate("admin", b"secret").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidPassword(_)));

            let credentials = BasicAuthCredentials::new("bruteforce".to_string(), "secret".to_string().into()).unwrap();
            let err = credentials.validate("admin", b"secret").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        #[test]
        fn it_rejects_credentials_with_an_empty_username() {
            let err = BasicAuthCredentials::new("".to_string(), "bruteforce".to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        #[test]
        fn it_rejects_over_sized_credentials() {
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();

            let username = [0u8; MAX_USERNAME_LEN / 2].to_hex();
            let credentials = BasicAuthCredentials::new(username.clone(), hashed_password.to_string().into()).unwrap();
            credentials.validate(&username, b"secret").unwrap();

            let username = [0u8; MAX_USERNAME_LEN / 2 + 1].to_hex();
            let err = BasicAuthCredentials::new(username, hashed_password.to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        // This unit test runs a series of iterations (defined by ITERATIONS), each of which compares invalid short and
        // long usernames with the actual username and measures the execution time.
        // For each iteration, it generates random invalid short and long usernames, with the long usernames being at
        // the maximum allowed length, measures the execution time for comparisons and calculates the variance between
        // the execution times for the three types of comparisons. Finally, it asserts that the minimum variance is
        // less than 10% (chosen to be robust for running the unit test with CI), indicating that the function behaves
        // within acceptable constant-time constraints.
        //
        // By testing the function with both short and long usernames, as well as the actual username, we can ensure
        // that the function performs consistently and doesn't introduce timing vulnerabilities. The variance analysis
        // provides a quantitative measure of the function's performance.
        //
        // Some consecutive results running in release mode on a Core i7-12700H (with no other processes running):
        //
        // Minimum variance:                          0.46632 %
        // Average variance:                          9.91276 %
        // Average short username time:               1.60382 microseconds
        // Average long username time:                1.55745 microseconds
        // Average actual username time:              1.56658 microseconds
        //
        // Minimum variance:                          0.37012 %
        // Average variance:                          5.04858 %
        // Average short username time:               1.2587 microseconds
        // Average long username time:                1.25306 microseconds
        // Average actual username time:              1.25922 microseconds
        //
        // Minimum variance:                          0.20113 %
        // Average variance:                          4.10358 %
        // Average short username time:               1.24704 microseconds
        // Average long username time:                1.24829 microseconds
        // Average actual username time:              1.25298 microseconds
        //
        // Some consecutive results running in release mode on a Core i7-12700H (while entire CPU fully stressed):
        //
        // Minimum variance:                          0.80897 %
        // Average variance:                          10.55519 %
        // Average short username time:               2.85889 microseconds
        // Average long username time:                2.79088 microseconds
        // Average actual username time:              2.80302 microseconds
        //
        // Minimum variance:                          1.70523 %
        // Average variance:                          11.29322 %
        // Average short username time:               2.85809 microseconds
        // Average long username time:                2.8062 microseconds
        // Average actual username time:              2.86813 microseconds
        //
        // Minimum variance:                          0.98332 %
        // Average variance:                          11.96301 %
        // Average short username time:               2.92986 microseconds
        // Average long username time:                2.83866 microseconds
        // Average actual username time:              2.87642 microseconds
        //
        // Minimum variance:                          0.55891 %
        // Average variance:                          10.46973 %
        // Average short username time:               2.93612 microseconds
        // Average long username time:                2.86887 microseconds
        // Average actual username time:              2.9968 microseconds
        //
        #[test]
        fn it_compares_user_names_in_constant_time() {
            #[allow(clippy::cast_possible_truncation)]
            fn round_to_6_decimals(num: f64) -> f64 {
                ((num * 100000.0) as u128) as f64 / 100000.0
            }

            const ITERATIONS: usize = 100;
            let mut variances = Vec::with_capacity(ITERATIONS);
            let mut short = Vec::with_capacity(ITERATIONS);
            let mut long = Vec::with_capacity(ITERATIONS);
            let mut actual = Vec::with_capacity(ITERATIONS);
            const COUNTS: usize = 2500;
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            for i in 1..=ITERATIONS {
                let username_actual = "admin";
                let credentials =
                    BasicAuthCredentials::new(username_actual.to_string(), hashed_password.to_string().into()).unwrap();
                assert!(credentials.constant_time_compare_username(username_actual));
                assert!(!credentials.constant_time_compare_username(""));

                let mut short_usernames = Vec::with_capacity(COUNTS);
                let mut long_usernames = Vec::with_capacity(COUNTS);
                for _ in 0..COUNTS {
                    let mut bytes_long = [0u8; MAX_USERNAME_LEN / 2];
                    let mut rng = rand::thread_rng();
                    rng.fill_bytes(&mut bytes_long);
                    let username = bytes_long.to_vec().to_hex();
                    long_usernames.push(username);
                    let mut bytes_short = [0u8; 12];
                    bytes_short.copy_from_slice(&bytes_long[..12]);
                    let username = bytes_short.to_vec().to_hex();
                    short_usernames.push(username);
                }

                let start = Instant::now();
                for short in &short_usernames {
                    let res = credentials.constant_time_compare_username(short);
                    assert!(!res);
                }
                let time_taken_1 = start.elapsed().as_micros();

                let start = Instant::now();
                for long in &long_usernames {
                    let res = credentials.constant_time_compare_username(long);
                    assert!(!res);
                }
                let time_taken_2 = start.elapsed().as_micros();

                let start = Instant::now();
                for _ in 0..COUNTS {
                    let res = credentials.constant_time_compare_username(username_actual);
                    assert!(res);
                }
                let time_taken_3 = start.elapsed().as_micros();

                let max_time = max(time_taken_1, max(time_taken_2, time_taken_3));
                let min_time = min(time_taken_1, min(time_taken_2, time_taken_3));
                let variance = round_to_6_decimals((max_time - min_time) as f64 / min_time as f64 * 100.0);
                variances.push(variance);
                short.push(time_taken_1);
                long.push(time_taken_2);
                actual.push(time_taken_3);

                // The use of sleep between iterations helps ensure that the tests are run under different conditions,
                // simulating real-world scenarios.
                if i < ITERATIONS {
                    sleep(std::time::Duration::from_millis(100));
                }
            }

            let min_variance = variances.iter().min_by(|x, y| x.partial_cmp(y).unwrap()).unwrap();
            let avg_variance = round_to_6_decimals(variances.iter().sum::<f64>() / variances.len() as f64);
            let avg_short = round_to_6_decimals(short.iter().sum::<u128>() as f64 / short.len() as f64 / COUNTS as f64);
            let avg_long = round_to_6_decimals(long.iter().sum::<u128>() as f64 / long.len() as f64 / COUNTS as f64);
            let avg_actual =
                round_to_6_decimals(actual.iter().sum::<u128>() as f64 / actual.len() as f64 / COUNTS as f64);
            println!("Minimum variance:                          {} %", min_variance);
            println!("Average variance:                          {} %", avg_variance);
            println!("Average short username time:               {} microseconds", avg_short);
            println!("Average long username time:                {} microseconds", avg_long);
            println!("Average actual username time:              {} microseconds", avg_actual);
            assert!(*min_variance < 10.0);
        }
    }

    mod generate_header {
        use super::*;

        #[test]
        fn it_generates_a_valid_header() {
            let header = BasicAuthCredentials::generate_header("admin", b"secret").unwrap();
            let cred = BasicAuthCredentials::from_header(header.to_str().unwrap()).unwrap();
            assert_eq!(cred.user_name, "admin");
            assert_eq!(cred.password.reveal(), &b"secret"[..]);
        }
    }
}
