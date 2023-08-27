//  Copyright 2022. The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.
use argon2::{
    password_hash::{Decimal, SaltString},
    Argon2,
    PasswordHasher,
};
use rand::rngs::OsRng;
use zeroize::Zeroizing;

pub fn create_salted_hashed_password(password: &[u8]) -> argon2::password_hash::Result<Zeroizing<String>> {
    // Generate a 16-byte random salt
    let passphrase_salt = SaltString::generate(&mut OsRng);

    // Use the recommended OWASP parameters, which are not the default:
    // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
    let params = argon2::Params::new(
        46 * 1024, // m-cost: 46 MiB, converted to KiB
        1,         // t-cost
        1,         // p-cost
        None,      // output length: default
    )?;

    // Hash the password; this is placed in the configuration file
    // We explicitly use the recommended algorithm and version due to the API
    let hashed_password = Argon2::default().hash_password_customized(
        password,
        Some(argon2::Algorithm::Argon2id.ident()),
        Some(argon2::Version::V0x13 as Decimal), // for some reason we need to use the numerical representation here
        params,
        &passphrase_salt,
    )?;

    Ok(Zeroizing::new(hashed_password.to_string()))
}
