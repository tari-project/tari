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

use std::{
    borrow::Cow,
    cmp::min,
    ops::{BitAnd, BitOr, Deref, Not},
    string::FromUtf8Error,
};

use argon2::{password_hash::Encoding, Argon2, PasswordHash, PasswordVerifier};
use rand::RngCore;
use subtle::{Choice, ConstantTimeEq};
use tari_utilities::{ByteArray, SafePassword};
use tonic::metadata::{errors::InvalidMetadataValue, Ascii, MetadataValue};
use zeroize::{Zeroize, Zeroizing};

const MAX_USERNAME_LEN: usize = 256;

/// Implements [RFC 2617](https://www.ietf.org/rfc/rfc2617.txt) by allowing authentication of provided credentials
#[derive(Debug)]
pub struct BasicAuthCredentials {
    /// The username bytes length
    pub user_name_bytes_length: usize,
    /// The username in bytes representation for constant time comparison
    pub user_name_bytes: [u8; MAX_USERNAME_LEN],
    /// The hashed password
    pub phc_password_hash: SafePassword,
    /// Random bytes to help with constant time username comparison
    pub random_bytes: [u8; MAX_USERNAME_LEN],
}

impl BasicAuthCredentials {
    /// Creates a new `Credentials` instance from a username and password (PHC string bytes).
    pub fn new(user_name: String, phc_password_hash: SafePassword) -> Result<Self, BasicAuthError> {
        // Validate the username is well formed
        if user_name.as_bytes().len() > MAX_USERNAME_LEN {
            return Err(BasicAuthError::InvalidUsername);
        }
        // Validate the password is a well formed byte representation of a PHC string
        let bytes = phc_password_hash.reveal().to_vec();
        let _parse_result = PasswordHash::parse(&String::from_utf8(bytes)?, Encoding::B64)?;
        // Random bytes are used for constant time username comparison to ensure that the compiler does not do any
        // funny optimizations and to ensure that comparison for the same username for every new credentials instance
        // forces a different bitwise comparison.
        let mut random_bytes = [0u8; MAX_USERNAME_LEN];
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut random_bytes);
        // Prepare the username bytes for constant time comparison ahead of time
        let bytes = user_name.as_bytes();
        let mut user_name_bytes = [0u8; MAX_USERNAME_LEN];
        user_name_bytes[0..bytes.len()].clone_from_slice(bytes);
        user_name_bytes[bytes.len()..MAX_USERNAME_LEN].clone_from_slice(&random_bytes[bytes.len()..MAX_USERNAME_LEN]);

        Ok(Self {
            user_name_bytes_length: user_name.as_bytes().len(),
            user_name_bytes,
            phc_password_hash,
            random_bytes,
        })
    }

    /// Parses the contents of an HTTP Authorization (Basic) header into a username and password
    /// These can be used later to validate against `BasicAuthCredentials`
    /// The input must be of the form `Basic base64(username:password)`
    pub fn parse_header(auth_header: &str) -> Result<(String, SafePassword), BasicAuthError> {
        // Check that the authentication type is `Basic`
        let (auth_type, encoded_credentials) = auth_header
            .split_once(' ')
            .ok_or(BasicAuthError::InvalidAuthorizationHeader)?;

        if auth_type.to_lowercase() != "basic" {
            return Err(BasicAuthError::InvalidScheme(auth_type.to_string()));
        }

        // Decode the credentials using base64
        let decoded = base64::decode(encoded_credentials)?;
        let as_utf8 = Zeroizing::new(String::from_utf8(decoded)?);

        // Parse the username and password, which must be separated by a colon
        if let Some((user_name, password)) = as_utf8.split_once(':') {
            return Ok((user_name.into(), password.into()));
        }

        Err(BasicAuthError::InvalidAuthorizationHeader)
    }

    // This function provides a constant time comparison of the given username with the registered username.
    fn constant_time_verify_username(&self, username: &str) -> Choice {
        // The username is valid if the lengths are equal and the length is not greater than the maximum allowed length;
        // any error here will only be factored in after the bitwise comparison has been done to force constant time.
        let bytes = username.as_bytes();
        let valid_username = (Choice::from(u8::from(self.user_name_bytes_length != bytes.len()))
            .bitor(Choice::from(u8::from(bytes.len() > MAX_USERNAME_LEN))))
        .not();

        // We start with an empty default buffer
        let mut compare_bytes = [0u8; MAX_USERNAME_LEN];

        // Add the username bytes to the buffer
        let bytes_len_clipped = min(bytes.len(), MAX_USERNAME_LEN);
        compare_bytes[0..bytes_len_clipped].clone_from_slice(&bytes[..bytes_len_clipped]);

        // The remaining bytes are padded afterwards (and not initialized at the start) to ensure that this function
        // always does the same amount of work irrespective of the username length.
        compare_bytes[bytes.len()..MAX_USERNAME_LEN]
            .clone_from_slice(&self.random_bytes[bytes.len()..MAX_USERNAME_LEN]);

        // Perform the bitwise comparison and combine the result with the valid username result.
        // The use of `Choice` logic here is by design to hide the boolean logic from compiler optimizations.
        self.user_name_bytes.ct_eq(&compare_bytes).bitand(valid_username)
    }

    /// Validates the given username and password against the registered username and password. The function will always
    /// do the same amount of work irrespective if the username or password is correct or not. This is to prevent timing
    /// attacks. Also, no distinction is made between a non-existent username or an incorrect password in the error
    /// that is returned.
    pub fn constant_time_validate(&self, username: &str, password: &SafePassword) -> Result<(), BasicAuthError> {
        let valid_username = self.constant_time_verify_username(username);

        // These bytes can leak if the password is not utf-8, but since argon encoding is utf-8 the given
        // password must be incorrect if conversion to utf-8 fails.
        let bytes = self.phc_password_hash.reveal().to_vec();
        let str_password = Zeroizing::new(String::from_utf8(bytes)?);
        let header_password = PasswordHash::parse(&str_password, Encoding::B64)?;
        let valid_password = Choice::from(u8::from(
            Argon2::default()
                .verify_password(password.reveal(), &header_password)
                .is_ok(),
        ));

        // The use of `Choice` logic here is by design to hide the boolean logic from compiler optimizations.
        if valid_username.bitand(valid_password).into() {
            Ok(())
        } else {
            Err(BasicAuthError::InvalidUsernameOrPassword)
        }
    }

    /// Generates a `Basic` HTTP Authorization header value from the given username and password.
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
            let (username, password) = BasicAuthCredentials::parse_header("Basic YWRtaW46c2VjcmV0").unwrap();
            assert_eq!(username, "admin".to_string());
            assert_eq!(password.reveal(), b"secret");
        }

        #[test]
        fn it_rejects_header_without_basic_scheme() {
            let err = BasicAuthCredentials::parse_header(" YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "");
            } else {
                panic!("Unexpected error: {:?}", err);
            };
            let err = BasicAuthCredentials::parse_header("Cookie YWRtaW46c2VjcmV0").unwrap_err();
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
            // Typical username
            let credentials =
                BasicAuthCredentials::new("admin".to_string(), hashed_password.to_string().into()).unwrap();
            credentials
                .constant_time_validate("admin", &SafePassword::from("secret".to_string()))
                .unwrap();
            // Empty username is also fine
            let credentials = BasicAuthCredentials::new("".to_string(), hashed_password.to_string().into()).unwrap();
            credentials
                .constant_time_validate("", &SafePassword::from("secret".to_string()))
                .unwrap();
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

            // Wrong password
            let err = credentials
                .constant_time_validate("admin", &SafePassword::from("bruteforce".to_string()))
                .unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));

            // Wrong username
            let err = credentials
                .constant_time_validate("wrong_user", &SafePassword::from("secret".to_string()))
                .unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));

            // Wrong username and password
            let err = credentials
                .constant_time_validate("wrong_user", &SafePassword::from("bruteforce".to_string()))
                .unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));
        }

        #[test]
        fn it_rejects_registering_over_sized_username_credentials() {
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();

            // Maximum length username is ok
            let username = [0u8; MAX_USERNAME_LEN / 2].to_hex();
            assert!(BasicAuthCredentials::new(username, hashed_password.to_string().into()).is_ok());

            // Empty length username is ok
            let username = [].to_hex();
            assert!(BasicAuthCredentials::new(username, hashed_password.to_string().into()).is_ok());

            // Do not accept username that is too long
            let username = [0u8; MAX_USERNAME_LEN / 2 + 1].to_hex();
            let err = BasicAuthCredentials::new(username, hashed_password.to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        // This unit test asserts that the minimum variance is less than 10% (chosen to be robust for running the unit
        // test with CI), indicating that the function behaves within acceptable constant-time constraints.
        //
        // Some consecutive results running in release mode on a Core i7-12700H (with no other processes running):
        //
        // Minimum variance:                          0.12574 %
        // Average variance:                          5.51684 %
        // Average short username time:               1.2922 microseconds
        // Average long username time:                1.27837 microseconds
        // Average actual username time:              1.28199 microseconds
        //
        // Minimum variance:                          0.06754 %
        // Average variance:                          3.64757 %
        // Average short username time:               1.27054 microseconds
        // Average long username time:                1.26604 microseconds
        // Average actual username time:              1.2615 microseconds
        //
        // Minimum variance:                          0.13508 %
        // Average variance:                          5.97782 %
        // Average short username time:               1.26488 microseconds
        // Average long username time:                1.27111 microseconds
        // Average actual username time:              1.26225 microseconds
        //
        // Some consecutive results running in release mode on a Core i7-12700H (while entire CPU fully stressed):
        //
        // Minimum variance:                          0.7276 %
        // Average variance:                          7.50704 %
        // Average short username time:               1.7147 microseconds
        // Average long username time:                1.6953 microseconds
        // Average actual username time:              1.6494 microseconds
        //
        // Minimum variance:                          0.41439 %
        // Average variance:                          7.17822 %
        // Average short username time:               1.80315 microseconds
        // Average long username time:                1.75904 microseconds
        // Average actual username time:              1.71591 microseconds
        //
        // Minimum variance:                          0.44736 %
        // Average variance:                          5.48951 %
        // Average short username time:               1.81177 microseconds
        // Average long username time:                1.78756 microseconds
        // Average actual username time:              1.73798 microseconds
        //
        #[test]
        fn it_compares_user_names_in_constant_time() {
            // Enable flag `do_performance_testing` to run performance tests; for regular CI runs, this flag should be
            // `false` otherwise the test will fail.
            // Notes:
            // - The `assert!(!do_performance_testing);` at the end of the test will cause a panic on CI if the flag is
            //   enabled, if it is enabled it will allow results to be printed when running in release mode.
            // - For CI (flag disabled), we are only interested if the functional test pass, thus 1 iteration completed
            //   successfully.
            let do_performance_testing = false;

            #[allow(clippy::cast_possible_truncation)]
            fn round_to_6_decimals(num: f64) -> f64 {
                ((num * 100000.0) as u128) as f64 / 100000.0
            }

            const ITERATIONS: usize = 250;
            let mut variances = Vec::with_capacity(ITERATIONS);
            let mut short = Vec::with_capacity(ITERATIONS);
            let mut long = Vec::with_capacity(ITERATIONS);
            let mut actual = Vec::with_capacity(ITERATIONS);
            // This value should be chosen to comply with:
            // - Small enough to ensure a single iteration does not take too long.
            // - Large enough to enable proper time measurement; executing the function that many times should be
            //   measurable, thus > micro seconds in this case.
            const COUNTS: usize = 2500;
            let username_actual = "admin";
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            let mut test_runs = 0;
            for i in 1..=ITERATIONS {
                let credentials =
                    BasicAuthCredentials::new(username_actual.to_string(), hashed_password.to_string().into()).unwrap();
                assert!(bool::from(credentials.constant_time_verify_username(username_actual)));
                assert!(!bool::from(credentials.constant_time_verify_username("")));

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
                    let res = credentials.constant_time_verify_username(short);
                    assert!(!bool::from(res));
                }
                let time_taken_1 = start.elapsed().as_micros();

                let start = Instant::now();
                for long in &long_usernames {
                    let res = credentials.constant_time_verify_username(long);
                    assert!(!bool::from(res));
                }
                let time_taken_2 = start.elapsed().as_micros();

                let start = Instant::now();
                for _ in 0..COUNTS {
                    let res = credentials.constant_time_verify_username(username_actual);
                    assert!(bool::from(res));
                }
                let time_taken_3 = start.elapsed().as_micros();

                let max_time = max(time_taken_1, max(time_taken_2, time_taken_3));
                let min_time = min(time_taken_1, min(time_taken_2, time_taken_3));
                let variance = round_to_6_decimals((max_time - min_time) as f64 / min_time as f64 * 100.0);
                variances.push(variance);
                short.push(time_taken_1);
                long.push(time_taken_2);
                actual.push(time_taken_3);

                test_runs += 1;
                if !do_performance_testing {
                    break;
                }

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
            println!("Test runs:                                 {}", test_runs);
            println!("Minimum variance:                          {} %", min_variance);
            println!("Average variance:                          {} %", avg_variance);
            println!("Average short username time:               {} microseconds", avg_short);
            println!("Average long username time:                {} microseconds", avg_long);
            println!("Average actual username time:              {} microseconds", avg_actual);

            // This is to make sure we do not run performance tests on CI.
            assert!(!do_performance_testing);
        }

        // This unit test asserts that the minimum variance is less than 10% (chosen to be robust for running the unit
        // test with CI), indicating that the function behaves within acceptable constant-time constraints.
        //
        // Some consecutive results running in release mode on a Core i7-12700H (with no other processes running):
        //
        // Minimum variance:                          0.43731 %
        // Average variance:                          2.66751 %
        // Average short username time:               35.04999 microseconds
        // Average long username time:                34.95 microseconds
        // Average actual username time:              34.9 microseconds
        //
        // Minimum variance:                          1.1713 %
        // Average variance:                          2.82044 %
        // Average short username time:               34.605 microseconds
        // Average long username time:                34.69 microseconds
        // Average actual username time:              34.67499 microseconds
        //
        // Minimum variance:                          0.9929 %
        // Average variance:                          2.35816 %
        // Average short username time:               35.285 microseconds
        // Average long username time:                35.285 microseconds
        // Average actual username time:              34.94 microseconds
        //
        // Some consecutive results running in release mode on a Core i7-12700H (while entire CPU fully stressed):
        //
        // Minimum variance:                          0.43668 %
        // Average variance:                          1.61542 %
        // Average short username time:               68.45 microseconds
        // Average long username time:                68.245 microseconds
        // Average actual username time:              68.81 microseconds
        //
        // Minimum variance:                          0.86268 %
        // Average variance:                          1.58273 %
        // Average short username time:               69.925 microseconds
        // Average long username time:                70.34999 microseconds
        // Average actual username time:              69.965 microseconds
        //
        // Minimum variance:                          0.4961 %
        // Average variance:                          1.61912 %
        // Average short username time:               69.85499 microseconds
        // Average long username time:                70.08 microseconds
        // Average actual username time:              70.645 microseconds
        //
        #[test]
        fn it_compares_credentials_in_constant_time() {
            // Enable flag `do_performance_testing` to run performance tests; for regular CI runs, this flag should be
            // `false` otherwise the test will fail.
            // Notes:
            // - The `assert!(!do_performance_testing);` at the end of the test will cause a panic on CI if the flag is
            //   enabled, if it is enabled it will allow results to be printed when running in release mode.
            // - For CI (flag disabled), we are only interested if the functional test pass, thus 1 iteration completed
            //   successfully.
            // - Running this specific test in debug mode is ~100x slower when compared to release mode.
            let do_performance_testing = false;

            #[allow(clippy::cast_possible_truncation)]
            fn round_to_6_decimals(num: f64) -> f64 {
                ((num * 100000.0) as u128) as f64 / 100000.0
            }

            const ITERATIONS: usize = 250;
            let mut variances = Vec::with_capacity(ITERATIONS);
            let mut short = Vec::with_capacity(ITERATIONS);
            let mut long = Vec::with_capacity(ITERATIONS);
            let mut actual = Vec::with_capacity(ITERATIONS);
            // This value should be chosen to comply with:
            // - Small enough to ensure a single iteration does not take too long.
            // - Large enough to enable proper time measurement; executing the function that many times should be
            //   measurable, thus > milli seconds in this case.
            const COUNTS: usize = 10;
            let username_actual = "admin";
            let hashed_password = create_salted_hashed_password(b"secret").unwrap();
            let mut test_runs = 0;
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
                    let res = credentials.constant_time_validate(short, &SafePassword::from("bruteforce".to_string()));
                    assert!(res.is_err());
                }
                let time_taken_1 = start.elapsed().as_millis();

                let start = Instant::now();
                for long in &long_usernames {
                    let res = credentials.constant_time_validate(long, &SafePassword::from("bruteforce".to_string()));
                    assert!(res.is_err());
                }
                let time_taken_2 = start.elapsed().as_millis();

                let start = Instant::now();
                for _ in 0..COUNTS {
                    let res =
                        credentials.constant_time_validate(username_actual, &SafePassword::from("secret".to_string()));
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

                test_runs += 1;
                if !do_performance_testing {
                    break;
                }

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
            println!("Test runs:                                 {}", test_runs);
            println!("Minimum variance:                          {} %", min_variance);
            println!("Average variance:                          {} %", avg_variance);
            println!("Average short username time:               {} microseconds", avg_short);
            println!("Average long username time:                {} microseconds", avg_long);
            println!("Average actual username time:              {} microseconds", avg_actual);

            // This is to make sure we do not run performance tests on CI.
            assert!(!do_performance_testing);
        }
    }

    mod generate_header {
        use super::*;

        #[test]
        fn it_generates_a_valid_header() {
            let header = BasicAuthCredentials::generate_header("admin", b"secret").unwrap();
            let (username, password) = BasicAuthCredentials::parse_header(header.to_str().unwrap()).unwrap();
            assert_eq!(username, "admin".to_string());
            assert_eq!(password.reveal(), b"secret");
        }
    }
}
