//  Copyright 2021, The Tari Project
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

/// This macro has three different patterns that it can match against based on the syntax provided.
///
/// The first pattern matches when the macro is called with an identifier followed by parentheses and optional arguments
/// ($token($($args:tt)*)). This pattern is useful when you want to create a covenant with some specific arguments.
///
/// The second pattern matches when the macro is called with just an identifier followed by empty parentheses
/// ($token()). This pattern is useful when you want to create a covenant without any arguments.
///
/// The third pattern matches when the macro is called with empty parentheses (()). This pattern is used when you want
/// to create a covenant with no additional arguments. Simple syntax for expressing covenants.
///
/// ```rust,ignore
/// // Before height 42, this may only be spent into an output with flag 8 (NON_FUNGIBLE)
/// let covenant = covenant!(or(absolute_height(@uint(42)), field_eq(@field::features_flags, @uint(8))));
/// covenant.execute(...)?;
/// ```
#[macro_export]
macro_rules! covenant {
    ($token:ident($($args:tt)*)) => {{
        let mut covenant = $crate::covenants::Covenant::new();
        $crate::__covenant_inner!(@ { covenant } $token($($args)*));
        covenant
    }};

    ($token:ident()) => {{
        let mut covenant = $crate::covenants::Covenant::new();
        $crate::__covenant_inner!(@ { covenant } $token());
        covenant
    }};

    () => { $crate::covenants::Covenant::new() };
}

#[macro_export]
// Macro for different pattern matching rules:
//
//     1. @ { $covenant:ident } => {}: This rule matches an empty input and does nothing.
//
//     2. @ { $covenant:ident } $token:ident() $(,)? => { ... }: This rule matches a token followed by empty
//         parentheses. It invokes a method push_token on the covenant object with the generated
//         CovenantToken::$token().
//
//     3. @ { $covenant:ident } @field::$field:ident, $($tail:tt)* => { ... }: This rule matches a token @field::$field
//         followed by a comma-separated list of tokens. It invokes a method push_token on the covenant object with the
//         generated CovenantToken::field($crate::covenants::OutputField::$field()). Then it recursively calls
//         __covenant_inner! with the remaining tokens.
//
//     4. @ { $covenant:ident } @field::$field:ident $(,)? => { ... }: This rule matches a single @field::$field token
//         followed by an optional comma. It delegates to the previous rule to handle the token.
//
//     5. @ { $covenant:ident } @fields($(@field::$field:ident),+ $(,)?)) => { ... }: This rule matches @fields
//          followed by a comma-separated list of @field::$field tokens wrapped in parentheses. It invokes a method
//          push_token on the covenant object with the generated CovenantToken::fields vector containing
//          OutputField::$field instances. It then recursively calls __covenant_inner! with the remaining tokens.
//
//     6. @ { $covenant:ident } @fields($(@field::$field:ident),+ $(,)?), $($tail:tt)* => { ... }: This rule is similar
//          to the previous one but allows for additional tokens after the @fields list. It behaves similarly by
//          generating the CovenantToken::fields vector and recursively calling __covenant_inner! with the remaining
//          tokens.
//
//     This macro pattern is called a tt-muncher (tee hee)
macro_rules! __covenant_inner {
    (@ { $covenant:ident }) => {};

    // token()
    (@ { $covenant:ident } $token:ident() $(,)?) => {
        $covenant.push_token($crate::covenants::CovenantToken::$token());
    };

    // @field::name, ...
    (@ { $covenant:ident } @field::$field:ident, $($tail:tt)*) => {
        $covenant.push_token($crate::covenants::CovenantToken::field($crate::covenants::OutputField::$field()));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @field::name
    (@ { $covenant:ident } @field::$field:ident $(,)?) => {
        $crate::__covenant_inner!(@ { $covenant } @field::$field,)
    };

    // @fields(@field::name, ...)
    (@ { $covenant:ident } @fields($(@field::$field:ident),+ $(,)?)) => {
        $crate::__covenant_inner!(@ { $covenant } @fields($(@field::$field),+),)
    };

    // @fields(@field::name, ...), ...
    (@ { $covenant:ident } @fields($(@field::$field:ident),+ $(,)?), $($tail:tt)*) => {
        $covenant.push_token($crate::covenants::CovenantToken::fields(vec![
            $($crate::covenants::OutputField::$field()),+
        ]));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @covenant_lit(...), ...
    (@ { $covenant:ident } @covenant_lit($($inner:tt)*), $($tail:tt)*) => {
        let inner = $crate::covenant!($($inner)*);
        $covenant.push_token($crate::covenants::CovenantToken::covenant(inner));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @covenant_lit(...)
    (@ { $covenant:ident } @covenant_lit($($inner:tt)*) $(,)?) => {
        $crate::__covenant_inner!(@ { $covenant } @covenant_lit($($inner)*),)
    };

    // @output_type(expr1), ...
    (@ { $covenant:ident } @output_type($arg:expr $(,)?), $($tail:tt)*) => {
        use $crate::transactions::transaction_components::OutputType::*;
        $covenant.push_token($crate::covenants::CovenantToken::output_type($arg));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @arg(expr1, expr2, ...), ...
    (@ { $covenant:ident } @$arg:ident($($args:expr),* $(,)?), $($tail:tt)*) => {
        $covenant.push_token($crate::covenants::CovenantToken::$arg($($args),*));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @arg(expr1, expr2, ...)
    (@ { $covenant:ident } @$arg:ident($($args:expr),* $(,)?) $(,)?) => {
        $crate::__covenant_inner!(@ { $covenant } @$arg($($args),*),)
    };

    // token(), ...
    (@ { $covenant:ident } $token:ident(), $($tail:tt)*) => {
        $covenant.push_token($crate::covenants::CovenantToken::$token());
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };
      // token(filter1, filter2, ...)
    (@ { $covenant:ident } $token:ident($($args:tt)+)) => {
        $crate::__covenant_inner!(@ { $covenant } $token($($args)+),)
    };

    // token(filter1, filter2, ...), ...
    (@ { $covenant:ident } $token:ident($($args:tt)+), $($tail:tt)*) => {
        $covenant.push_token($crate::covenants::CovenantToken::$token());
        $crate::__covenant_inner!(@ { $covenant } $($args)+ $($tail)*)
    };

    // token(...)
    (@ { $covenant:ident } $token:ident($($args:tt)+)) => {
        $covenant.push_token($crate::covenants::CovenantToken::$token());
        $crate::__covenant_inner!(@ { $covenant } $($args)+)
    };
}

#[cfg(test)]
mod test {
    use tari_common_types::types::PublicKey;
    use tari_script::script;
    use tari_test_utils::unpack_enum;
    use tari_utilities::{
        hex::{from_hex, Hex},
        ByteArray,
    };

    use crate::covenants::{arguments::CovenantArg, filters::CovenantFilter, token::CovenantToken, Covenant};

    #[test]
    fn simple() {
        let covenant = covenant!(identity());
        assert_eq!(covenant.tokens().len(), 1);
        assert!(matches!(
            covenant.tokens()[0],
            CovenantToken::Filter(CovenantFilter::Identity(_))
        ));
    }

    #[test]
    fn script() {
        let hash = "53563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd";
        let hash = {
            let mut buf = [0u8; 32];
            buf.copy_from_slice(from_hex(hash).unwrap().as_slice());
            buf
        };
        let dest_pk = PublicKey::from_hex("b0c1f788f137ba0cdc0b61e89ee43b80ebf5cca4136d3229561bf11eba347849").unwrap();
        let sender_pk = dest_pk.clone();
        let script = script!(HashSha256 PushHash(Box::new(hash)) Equal IfThen PushPubKey(Box::new(dest_pk)) Else CheckHeightVerify(100) PushPubKey(Box::new(sender_pk)) EndIf);
        let covenant = covenant!(field_eq(@field::script, @script(script.clone())));

        let decoded = Covenant::from_bytes(&mut covenant.to_bytes().as_bytes()).unwrap();
        assert_eq!(covenant, decoded);
        unpack_enum!(CovenantArg::TariScript(decoded_script) = decoded.tokens()[2].as_arg().unwrap());
        assert_eq!(script, *decoded_script);
    }
}
