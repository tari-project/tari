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
    fmt,
    ops::{BitAnd, BitOr, Deref, Not},
    string::FromUtf8Error,
};

use rand::Rng;
use subtle::{Choice, ConstantTimeEq};
use tari_utilities::SafePassword;
use tonic::metadata::{Ascii, MetadataValue, errors::InvalidMetadataValue};
use zeroize::{Zeroize, Zeroizing};

const MAX_USERNAME_LEN: usize = 256;
const MIN_PASSWORD_PAD_LEN: usize = 256;

fn prepare_padded_bytes<const N: usize>(input: &[u8]) -> ([u8; N], [u8; N]) {
    let mut random_bytes = [0u8; N];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut random_bytes);
    let mut padded_bytes = [0u8; N];

    let (prefix, suffix) = padded_bytes.split_at_mut(input.len());
    prefix.copy_from_slice(input);
    suffix.copy_from_slice(random_bytes.get(input.len()..N).expect("Already checked"));

    (padded_bytes, random_bytes)
}

fn prepare_padded_vec(input: &[u8], min_pad_len: usize) -> (Zeroizing<Vec<u8>>, Vec<u8>) {
    let pad_len = input.len().max(min_pad_len);
    let mut random_bytes = vec![0u8; pad_len];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut random_bytes);

    let mut padded_bytes = Zeroizing::new(vec![0u8; pad_len]);
    let (prefix, suffix) = padded_bytes.split_at_mut(input.len());
    prefix.copy_from_slice(input);
    suffix.copy_from_slice(random_bytes.get(input.len()..pad_len).expect("pad_len >= input.len()"));

    (padded_bytes, random_bytes)
}

fn constant_time_verify_slice<const N: usize>(
    expected_len: usize,
    expected_bytes: &[u8; N],
    random_padding: &[u8; N],
    candidate: &[u8],
) -> Choice {
    let valid_len = (Choice::from(u8::from(expected_len != candidate.len()))
        .bitor(Choice::from(u8::from(candidate.len() > N))))
    .not();

    let mut compare_bytes = [0u8; N];
    let bytes_len_clipped = min(candidate.len(), N);
    let (prefix, suffix) = compare_bytes.split_at_mut(bytes_len_clipped);
    prefix.copy_from_slice(
        candidate
            .get(..bytes_len_clipped)
            .expect("bytes_len_clipped is at most candidate.len()"),
    );
    suffix.copy_from_slice(
        random_padding
            .get(bytes_len_clipped..N)
            .expect("bytes_len_clipped is at most N"),
    );

    let eq = expected_bytes.ct_eq(&compare_bytes).bitand(valid_len);
    compare_bytes.zeroize();
    eq
}

fn constant_time_verify_padded(
    expected_len: usize,
    expected_bytes: &[u8],
    random_padding: &[u8],
    candidate: &[u8],
) -> Choice {
    let pad_len = expected_bytes.len();
    let valid_len = (Choice::from(u8::from(expected_len != candidate.len()))
        .bitor(Choice::from(u8::from(candidate.len() > pad_len))))
    .not();

    let mut eq = Choice::from(1u8);
    for (i, (&expected_byte, &pad_byte)) in expected_bytes.iter().zip(random_padding.iter()).enumerate() {
        let candidate_byte = candidate.get(i).copied().unwrap_or(pad_byte);
        eq = eq.bitand(expected_byte.ct_eq(&candidate_byte));
    }
    eq.bitand(valid_len)
}

/// Implements [RFC 2617](https://www.ietf.org/rfc/rfc2617.txt) by allowing authentication of provided credentials
#[derive(Clone)]
pub struct BasicAuthCredentials {
    /// The username bytes length
    pub user_name_bytes_length: usize,
    /// The username in bytes representation for constant time comparison
    pub user_name_bytes: [u8; MAX_USERNAME_LEN],
    /// Random bytes to help with constant time username comparison
    pub random_bytes: [u8; MAX_USERNAME_LEN],
    /// The password bytes length
    password_bytes_length: usize,
    /// The password in bytes representation for constant time comparison
    password_bytes: Zeroizing<Vec<u8>>,
    /// Random bytes to help with constant time password comparison
    password_random_bytes: Vec<u8>,
}

impl fmt::Debug for BasicAuthCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicAuthCredentials")
            .field("user_name_bytes_length", &self.user_name_bytes_length)
            .field("user_name_bytes", &self.user_name_bytes)
            .field("random_bytes", &self.random_bytes)
            .field("password_bytes_length", &"<redacted>")
            .field("password_bytes", &"<redacted>")
            .field("password_random_bytes", &"<redacted>")
            .finish()
    }
}

impl BasicAuthCredentials {
    /// Creates a new `BasicAuthCredentials` instance from a username and plaintext password.
    pub fn new(user_name: String, password: SafePassword) -> Result<Self, BasicAuthError> {
        // Validate the username is well formed
        if user_name.len() > MAX_USERNAME_LEN {
            return Err(BasicAuthError::InvalidUsername);
        }
        // Validate the password is well formed UTF-8
        let password_slice = password.reveal();
        if std::str::from_utf8(password_slice).is_err() {
            return Err(BasicAuthError::InvalidPassword);
        }

        // Prepare the username and password bytes for constant time comparison ahead of time.
        // Random bytes are used to ensure that the compiler does not do any funny optimizations and to ensure that
        // comparison for the same credential across instances forces a different bitwise comparison.
        let (user_name_bytes, random_bytes) = prepare_padded_bytes::<MAX_USERNAME_LEN>(user_name.as_bytes());
        let (password_bytes, password_random_bytes) = prepare_padded_vec(password_slice, MIN_PASSWORD_PAD_LEN);

        Ok(Self {
            user_name_bytes_length: user_name.len(),
            user_name_bytes,
            random_bytes,
            password_bytes_length: password_slice.len(),
            password_bytes,
            password_random_bytes,
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
        use base64::{Engine, prelude::BASE64_STANDARD};
        let decoded = BASE64_STANDARD.decode(encoded_credentials)?;
        let as_utf8 = Zeroizing::new(String::from_utf8(decoded)?);

        // Parse the username and password, which must be separated by a colon
        if let Some((user_name, password)) = as_utf8.split_once(':') {
            return Ok((user_name.into(), password.into()));
        }

        Err(BasicAuthError::InvalidAuthorizationHeader)
    }

    // This function provides a constant time comparison of the given username with the registered username.
    fn constant_time_verify_username(&self, username: &str) -> Choice {
        constant_time_verify_slice(
            self.user_name_bytes_length,
            &self.user_name_bytes,
            &self.random_bytes,
            username.as_bytes(),
        )
    }

    // This function provides a constant time comparison of the given password with the registered password.
    fn constant_time_verify_password(&self, password: &SafePassword) -> Choice {
        constant_time_verify_padded(
            self.password_bytes_length,
            &self.password_bytes,
            &self.password_random_bytes,
            password.reveal(),
        )
    }

    /// Validates the given username and password against the registered username and password. The function will always
    /// do the same amount of work irrespective if the username or password is correct or not. This is to prevent timing
    /// attacks. Also, no distinction is made between a non-existent username or an incorrect password in the error
    /// that is returned.
    pub fn constant_time_validate(&self, username: &str, password: &SafePassword) -> Result<(), BasicAuthError> {
        let valid_username = self.constant_time_verify_username(username);
        let valid_password = self.constant_time_verify_password(password);

        // The use of `Choice` logic here is by design to hide the boolean logic from compiler optimizations.
        if valid_username.bitand(valid_password).into() {
            Ok(())
        } else {
            Err(BasicAuthError::InvalidUsernameOrPassword)
        }
    }

    /// Generates a `Basic` HTTP Authorization header value from the given username and password.
    pub fn generate_header(username: &str, password: &[u8]) -> Result<MetadataValue<Ascii>, BasicAuthError> {
        use base64::{Engine, prelude::BASE64_STANDARD};
        let password_str = String::from_utf8_lossy(password);
        let token_str = Zeroizing::new(format!("{username}:{password_str}"));
        let mut token = BASE64_STANDARD.encode(token_str.deref());
        let header = format!("Basic {token}");
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
    #[error("Invalid password")]
    InvalidPassword,
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
    #[error("Invalid header value: {0}")]
    InvalidMetadataValue(#[from] InvalidMetadataValue),
}

#[cfg(test)]
mod tests {
    // Overflow in test code panics, which is the desired failure mode for a test.
    #![allow(clippy::arithmetic_side_effects)]
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
                panic!("Unexpected error: {err:?}");
            };
            let err = BasicAuthCredentials::parse_header("Cookie YWRtaW46c2VjcmV0").unwrap_err();
            if let BasicAuthError::InvalidScheme(s) = err {
                assert_eq!(s, "Cookie");
            } else {
                panic!("Unexpected error: {err:?}");
            };
        }
    }

    mod validate {
        use std::{cmp::max, thread::sleep};

        use tari_utilities::hex::Hex;

        use super::*;

        #[test]
        fn it_validates_for_matching_credentials() {
            // Typical username
            let credentials = BasicAuthCredentials::new("admin".to_string(), "secret".to_string().into()).unwrap();
            credentials
                .constant_time_validate("admin", &SafePassword::from("secret".to_string()))
                .unwrap();
            // Empty username is also fine
            let credentials = BasicAuthCredentials::new("".to_string(), "secret".to_string().into()).unwrap();
            credentials
                .constant_time_validate("", &SafePassword::from("secret".to_string()))
                .unwrap();
        }

        #[test]
        fn it_rejects_validating_mismatching_credentials() {
            let credentials = BasicAuthCredentials::new("admin".to_string(), "secret".to_string().into()).unwrap();

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
            // Maximum length username is ok
            let username = [0u8; MAX_USERNAME_LEN / 2].to_hex();
            assert!(BasicAuthCredentials::new(username, "secret".to_string().into()).is_ok());

            // Empty length username is ok
            let username = [].to_hex();
            assert!(BasicAuthCredentials::new(username, "secret".to_string().into()).is_ok());

            // Do not accept username that is too long
            let username = [0u8; MAX_USERNAME_LEN / 2 + 1].to_hex();
            let err = BasicAuthCredentials::new(username, "secret".to_string().into()).unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsername));
        }

        #[test]
        fn it_rejects_oversized_username_and_password_without_panicking() {
            let credentials = BasicAuthCredentials::new("admin".to_string(), "secret".to_string().into()).unwrap();

            let oversized_user = "a".repeat(MAX_USERNAME_LEN + 1);
            assert!(!bool::from(credentials.constant_time_verify_username(&oversized_user)));
            let err = credentials
                .constant_time_validate(&oversized_user, &SafePassword::from("secret".to_string()))
                .unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));

            let oversized_pass = SafePassword::from("p".repeat(MIN_PASSWORD_PAD_LEN + 1));
            assert!(!bool::from(credentials.constant_time_verify_password(&oversized_pass)));
            let err = credentials
                .constant_time_validate("admin", &oversized_pass)
                .unwrap_err();
            assert!(matches!(err, BasicAuthError::InvalidUsernameOrPassword));
        }

        #[test]
        fn it_supports_passwords_longer_than_256_bytes_and_redacts_debug_output() {
            let long_pass = "correct-horse-battery-staple-".repeat(12);
            assert!(long_pass.len() > MIN_PASSWORD_PAD_LEN);
            let credentials = BasicAuthCredentials::new("admin".to_string(), long_pass.clone().into()).unwrap();

            credentials
                .constant_time_validate("admin", &SafePassword::from(long_pass.clone()))
                .unwrap();

            let wrong_long_pass = format!("{long_pass}x");
            assert!(matches!(
                credentials.constant_time_validate("admin", &SafePassword::from(wrong_long_pass)),
                Err(BasicAuthError::InvalidUsernameOrPassword)
            ));

            let debug_repr = format!("{credentials:?}");
            assert!(
                !debug_repr.contains("correct-horse-battery-staple"),
                "Debug output must not leak password: {debug_repr}"
            );
            assert!(
                !debug_repr.contains(&format!("{:?}", long_pass.as_bytes())),
                "Debug output must not leak password bytes: {debug_repr}"
            );
            assert!(
                debug_repr.contains("<redacted>"),
                "Debug output must redact password fields: {debug_repr}"
            );
        }

        #[test]
        fn it_compares_user_names_in_constant_time() {
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
            const COUNTS: usize = 2500;
            let username_actual = "admin";
            let mut test_runs = 0;
            for i in 1..=ITERATIONS {
                let credentials =
                    BasicAuthCredentials::new(username_actual.to_string(), "secret".to_string().into()).unwrap();
                assert!(bool::from(credentials.constant_time_verify_username(username_actual)));
                assert!(!bool::from(credentials.constant_time_verify_username("")));

                let mut short_usernames = Vec::with_capacity(COUNTS);
                let mut long_usernames = Vec::with_capacity(COUNTS);
                for _ in 0..COUNTS {
                    let mut bytes_long = [0u8; MAX_USERNAME_LEN / 2];
                    let mut rng = rand::rng();
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
                let min_time = min(max(time_taken_1, 1), min(max(time_taken_2, 1), max(time_taken_3, 1)));
                let variance = round_to_6_decimals((max_time - min_time) as f64 / min_time as f64 * 100.0);
                variances.push(variance);
                short.push(time_taken_1);
                long.push(time_taken_2);
                actual.push(time_taken_3);

                test_runs += 1;
                if !do_performance_testing {
                    break;
                }

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
            println!("Test runs:                                 {test_runs}");
            println!("Minimum variance:                          {min_variance} %");
            println!("Average variance:                          {avg_variance} %");
            println!("Average short username time:               {avg_short} microseconds");
            println!("Average long username time:                {avg_long} microseconds");
            println!("Average actual username time:              {avg_actual} microseconds");

            assert!(!do_performance_testing);
        }

        #[test]
        fn it_compares_credentials_in_constant_time() {
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
            const COUNTS: usize = 2500;
            let username_actual = "admin";
            let mut test_runs = 0;
            for i in 1..=ITERATIONS {
                let credentials =
                    BasicAuthCredentials::new(username_actual.to_string(), "secret".to_string().into()).unwrap();

                let mut short_usernames = Vec::with_capacity(COUNTS);
                let mut long_usernames = Vec::with_capacity(COUNTS);
                for _ in 0..COUNTS {
                    let mut bytes_long = [0u8; MAX_USERNAME_LEN / 2];
                    let mut rng = rand::rng();
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
                let time_taken_1 = start.elapsed().as_micros();

                let start = Instant::now();
                for long in &long_usernames {
                    let res = credentials.constant_time_validate(long, &SafePassword::from("bruteforce".to_string()));
                    assert!(res.is_err());
                }
                let time_taken_2 = start.elapsed().as_micros();

                let start = Instant::now();
                for _ in 0..COUNTS {
                    let res =
                        credentials.constant_time_validate(username_actual, &SafePassword::from("secret".to_string()));
                    assert!(res.is_ok());
                }
                let time_taken_3 = start.elapsed().as_micros();

                let max_time = max(time_taken_1, max(time_taken_2, time_taken_3));
                let min_time = min(max(time_taken_1, 1), min(max(time_taken_2, 1), max(time_taken_3, 1)));
                let variance = round_to_6_decimals((max_time - min_time) as f64 / min_time as f64 * 100.0);
                variances.push(variance);
                short.push(time_taken_1);
                long.push(time_taken_2);
                actual.push(time_taken_3);

                test_runs += 1;
                if !do_performance_testing {
                    break;
                }

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
            println!("Test runs:                                 {test_runs}");
            println!("Minimum variance:                          {min_variance} %");
            println!("Average variance:                          {avg_variance} %");
            println!("Average short username time:               {avg_short} microseconds");
            println!("Average long username time:                {avg_long} microseconds");
            println!("Average actual username time:              {avg_actual} microseconds");

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
