// Copyright 2019, The Tari Project
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

/// Unpack the tuple or struct variant variables from an enum.
///
/// ```edition2018
/// # use tari_test_utils::unpack_enum;
///
/// #[derive(Debug)]
/// enum AnyEnum<'a> {
///     Tuple(u8, &'a str),
///     Struct { name: &'a str, age: u8 },
///     SingleVariant,
/// }
///
/// let e = AnyEnum::Tuple(123, "Hubert etc.");
/// unpack_enum!(AnyEnum::Tuple(age, name) = e);
/// assert_eq!(age, 123);
/// assert_eq!(name, "Hubert etc.");
///
/// let e = AnyEnum::Struct{age: 123, name: "Hubert etc."};
/// unpack_enum!(AnyEnum::Struct{ age, name } = e);
/// assert_eq!(age, 123);
/// assert_eq!(name, "Hubert etc.");
///
/// let e = AnyEnum::SingleVariant;
/// unpack_enum!(AnyEnum::SingleVariant = e);
///
/// // Will panic
/// // let e = AnyEnum::Tuple(123, "Hubert etc.");
/// // unpack_enum!(AnyEnum::SingleVariant = e);
/// ```
#[macro_export]
macro_rules! unpack_enum {
    ($($enum_key:ident)::+ { $($idents:tt),* } = $enum:expr) => {
        let ($($idents),+) =  match $enum {
            $($enum_key)::+{$($idents),+} => ($($idents),+),
            _ => panic!("Unexpected enum variant given to unpack_enum"),
        };
    };
    ($($enum_key:ident)::+ ( $($idents:tt),* ) = $enum:expr) => {
        let ($($idents),+) =  match $enum {
            $($enum_key)::+($($idents),+) => ($($idents),+),
            _ => panic!("Unexpected enum variant given to unpack_enum"),
        };
    };
    ($($enum_key:ident)::+ = $enum:expr) => {
        match $enum {
            $($enum_key)::+ => {},
            _ => panic!("Unexpected enum variant given to unpack_enum"),
        };
    };
}
