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

use std::{borrow::Cow, cmp::min, ops::Deref, string::FromUtf8Error};

use argon2::{password_hash::Encoding, Argon2, PasswordHash, PasswordVerifier};
use subtle::ConstantTimeEq;
use tari_utilities::{ByteArray, SafePassword};
use tonic::metadata::{errors::InvalidMetadataValue, Ascii, MetadataValue};
use zeroize::{Zeroize, Zeroizing};

use crate::authentication::salted_password::create_salted_hashed_password;

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
        // Validate the username is well formed
        if user_name.as_bytes().is_empty() || user_name.as_bytes().len() > MAX_USERNAME_LEN {
            return Err(BasicAuthError::InvalidUsername);
        }
        // Validate the password is well formed
        let bytes = password.reveal().to_vec();
        let str_password = Zeroizing::new(String::from_utf8(bytes)?);
        let _parse_result = PasswordHash::parse(&str_password, Encoding::B64)?;
        // We are happy with the username and password
        Ok(Self { user_name, password })
    }

    /// Creates a `Credentials` instance from a base64 `String`
    /// which must encode user credentials as `username:password`
    pub fn decode(auth_header_value: &str) -> Result<Self, BasicAuthError> {
        let decoded = base64::decode(auth_header_value)?;
        let as_utf8 = Zeroizing::new(String::from_utf8(decoded)?);

        if let Some((user_name, password)) = as_utf8.split_once(':') {
            let hashed_password = create_salted_hashed_password(password.as_bytes())?;
            let credentials = Self::new(user_name.into(), hashed_password.to_string().into())?;
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

    // This function provides a constant time comparison of the given username with the registered username.
    fn constant_time_compare_username(&self, username: &str) -> bool {
        let a_bytes = self.user_name.as_bytes();
        let b_bytes = username.as_bytes();

        let valid_username = !(b_bytes.is_empty() || b_bytes.len() > MAX_USERNAME_LEN);
        let b_bytes_len_clipped = min(b_bytes.len(), MAX_USERNAME_LEN);

        // Comparison bytes for both usernames are initialized to a large array of equal bytes.
        let mut a_compare_bytes = [7u8; MAX_USERNAME_LEN];
        let mut b_compare_bytes = a_compare_bytes;

        // Add the comparison bytes for the actual username to the respective comparison byte arrays.
        a_compare_bytes[0..a_bytes.len()].clone_from_slice(a_bytes);
        b_compare_bytes[0..b_bytes_len_clipped].clone_from_slice(&b_bytes[..b_bytes_len_clipped]);

        (a_compare_bytes.ct_eq(&b_compare_bytes).unwrap_u8() != 0) && valid_username
    }

    pub fn validate(&self, username: &str, password: &[u8]) -> Result<(), BasicAuthError> {
        let valid_username = self.constant_time_compare_username(username);

        // These bytes can leak if the password is not utf-8, but since argon encoding is utf-8 the given
        // password must be incorrect if conversion to utf-8 fails.
        let bytes = self.password.reveal().to_vec();
        let str_password = Zeroizing::new(String::from_utf8(bytes)?);
        let header_password = PasswordHash::parse(&str_password, Encoding::B64)?;
        let valid_password = Argon2::default().verify_password(password, &header_password).is_ok();

        if valid_username && valid_password {
            Ok(())
        } else {
            Err(BasicAuthError::InvalidUsernameOrPassword)
        }
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
    #[error("Invalid username or password")]
    InvalidUsernameOrPassword,
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
            assert!(credentials.validate("admin", b"secret").is_ok());
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
        use std::{
            cmp::{max, min},
            thread::sleep,
        };

        use rand::RngCore;
        use tari_utilities::{hex::Hex, ByteArray};

        use super::*;
        use crate::authentication::salted_password::create_salted_hashed_password;

        #[test]
        fn it_validates_for_matching_salted_credentials() {
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            let credentials =
                BasicAuthCredentials::new("admin".to_string(), hashed_password.to_string().into()).unwrap();
            credentials.validate("admin", b"secret").unwrap();
        }

        #[test]
        fn it_rejects_registering_unsalted_password_credentials() {
            let err = BasicAuthCredentials::new("admin".to_string(), "secret".to_string().into()).unwrap_err();
            assert!(matches!(
                err,
                BasicAuthError::InvalidPassword(argon2::password_hash::Error::PhcStringInvalid)
            ));
        }

        #[test]
        fn it_rejects_validating_mismatching_credentials() {
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            let credentials =
                BasicAuthCredentials::new("admin".to_string(), hashed_password.to_string().into()).unwrap();

            let err = credentials.validate("admin", b"bruteforce").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));

            let err = credentials.validate("wrong_user", b"secret").unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));
        }

        #[test]
        fn it_rejects_registering_empty_or_over_sized_username_credentials() {
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();

            let username = [0u8; MAX_USERNAME_LEN / 2].to_hex();
            assert!(BasicAuthCredentials::new(username, hashed_password.to_string().into()).is_ok());

            let username = [].to_hex();
            let err = BasicAuthCredentials::new(username, hashed_password.to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));

            let username = [0u8; MAX_USERNAME_LEN / 2 + 1].to_hex();
            let err = BasicAuthCredentials::new(username, hashed_password.to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        // This unit test asserts that the minimum variance is less than 10% (chosen to be robust for running the unit
        // test with CI), indicating that the function behaves within acceptable constant-time constraints.
        //
        // Some consecutive results running in release mode on a Core i7-12700H (with no other processes running):
        //
        // Minimum variance:                          0.247 %
        // Average variance:                          4.65738 %
        // Average short username time:               1.17486 microseconds
        // Average long username time:                1.17344 microseconds
        // Average actual username time:              1.18388 microseconds
        //
        // Minimum variance:                          0.10214 %
        // Average variance:                          4.32226 %
        // Average short username time:               1.1619 microseconds
        // Average long username time:                1.16591 microseconds
        // Average actual username time:              1.18157 microseconds
        //
        // Minimum variance:                          0.17953 %
        // Average variance:                          5.51519 %
        // Average short username time:               1.17974 microseconds
        // Average long username time:                1.19232 microseconds
        // Average actual username time:              1.18709 microseconds
        //
        // Some consecutive results running in release mode on a Core i7-12700H (while entire CPU fully stressed):
        //
        // Minimum variance:                          0.60357 %
        // Average variance:                          6.30167 %
        // Average short username time:               1.81708 microseconds
        // Average long username time:                1.77562 microseconds
        // Average actual username time:              1.74824 microseconds
        //
        // Minimum variance:                          0.28176 %
        // Average variance:                          6.47136 %
        // Average short username time:               1.8317 microseconds
        // Average long username time:                1.8304 microseconds
        // Average actual username time:              1.80362 microseconds
        //
        // Minimum variance:                          0.53593 %
        // Average variance:                          6.99394 %
        // Average short username time:               1.82322 microseconds
        // Average long username time:                1.81431 microseconds
        // Average actual username time:              1.78002 microseconds
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
            let username_actual = "admin";
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            for i in 1..=ITERATIONS {
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

        // This unit test asserts that the minimum variance is less than 10% (chosen to be robust for running the unit
        // test with CI), indicating that the function behaves within acceptable constant-time constraints.
        //
        // Some consecutive results running in release mode on a Core i7-12700H (with no other processes running):
        //
        // Minimum variance:                          0.43478 %
        // Average variance:                          2.08995 %
        // Average short username time:               34.580 microseconds
        // Average long username time:                34.315 microseconds
        // Average actual username time:              34.260 microseconds
        //
        // Minimum variance:                          0.43731 %
        // Average variance:                          1.77209 %
        // Average short username time:               34.560 microseconds
        // Average long username time:                34.755 microseconds
        // Average actual username time:              34.690 microseconds
        //
        // Minimum variance:                          0.43988 %
        // Average variance:                          1.61299 %
        // Average short username time:               34.33999 microseconds
        // Average long username time:                34.38500 microseconds
        // Average actual username time:              34.28500 microseconds
        //
        // Some consecutive results running in release mode on a Core i7-12700H (while entire CPU fully stressed):
        //
        // Minimum variance:                          0.30326 %
        // Average variance:                          2.29341 %
        // Average short username time:               64.87500 microseconds
        // Average long username time:                65.55499 microseconds
        // Average actual username time:              65.81000 microseconds
        //
        // Minimum variance:                          1.18168 %
        // Average variance:                          2.99206 %
        // Average short username time:               67.970 microseconds
        // Average long username time:                68.000 microseconds
        // Average actual username time:              68.005 microseconds
        //
        // Minimum variance:                          1.01083 %
        // Average variance:                          2.31316 %
        // Average short username time:               68.715 microseconds
        // Average long username time:                69.675 microseconds
        // Average actual username time:              69.715 microseconds
        //
        #[test]
        fn it_compares_credentials_in_constant_time() {
            #[allow(clippy::cast_possible_truncation)]
            fn round_to_6_decimals(num: f64) -> f64 {
                ((num * 100000.0) as u128) as f64 / 100000.0
            }

            const ITERATIONS: usize = 10;
            let mut variances = Vec::with_capacity(ITERATIONS);
            let mut short = Vec::with_capacity(ITERATIONS);
            let mut long = Vec::with_capacity(ITERATIONS);
            let mut actual = Vec::with_capacity(ITERATIONS);
            const COUNTS: usize = 20;
            let username_actual = "admin";
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            for i in 1..=ITERATIONS {
                let credentials =
                    BasicAuthCredentials::new(username_actual.to_string(), hashed_password.to_string().into()).unwrap();

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
                    let res = credentials.validate(short, b"bruteforce");
                    assert!(res.is_err());
                }
                let time_taken_1 = start.elapsed().as_millis();

                let start = Instant::now();
                for long in &long_usernames {
                    let res = credentials.validate(long, b"bruteforce");
                    assert!(res.is_err());
                }
                let time_taken_2 = start.elapsed().as_millis();

                let start = Instant::now();
                for _ in 0..COUNTS {
                    let res = credentials.validate(username_actual, b"secret");
                    assert!(res.is_ok());
                }
                let time_taken_3 = start.elapsed().as_millis();

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
            assert!(cred.validate("admin", b"secret").is_ok());
        }
    }
}
