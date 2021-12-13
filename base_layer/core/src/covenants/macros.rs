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

/// Simple syntax for expressing covenants.
///
/// ```rust,ignore
/// // Before height 42, this may only be spent into an output with flag 8 (NON_FUNGIBLE)
/// let covenant = covenant!(or(absolute_height(@uint(42)), field_eq(@field::features_flags, @uint(8)));
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

    // @covenant(...), ...
    (@ { $covenant:ident } @covenant($($inner:tt)*), $($tail:tt)*) => {
        let inner = $crate::covenant!($($inner)*);
        $covenant.push_token($crate::covenants::CovenantToken::covenant(inner));
        $crate::__covenant_inner!(@ { $covenant } $($tail)*)
    };

    // @covenant(...)
    (@ { $covenant:ident } @covenant($($inner:tt)*) $(,)?) => {
        $crate::__covenant_inner!(@ { $covenant } @covenant($($inner)*),)
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
    use tari_crypto::script;
    use tari_test_utils::unpack_enum;
    use tari_utilities::hex::{from_hex, Hex};

    use crate::{
        consensus::{ConsensusDecoding, ToConsensusBytes},
        covenants::{arguments::CovenantArg, filters::CovenantFilter, token::CovenantToken, Covenant},
    };

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
    fn fields() {
        let covenant =
            covenant!(and(identity(), fields_preserved(@fields(@field::commitment, @field::sender_offset_public_key))));
        assert_eq!(covenant.to_consensus_bytes().to_hex(), "21203108020002");
    }

    #[test]
    fn hash() {
        let hash_str = "53563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd";
        let hash_vec = from_hex(hash_str).unwrap();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_vec.as_slice());
        let covenant = covenant!(output_hash_eq(@hash(hash.clone())));
        assert_eq!(covenant.to_consensus_bytes().to_hex(), format!("3001{}", hash_str));

        let covenant = covenant!(and(
            identity(),
            or(
                identity(),
                fields_preserved(@hash(hash),)
            )
        ));
        assert_eq!(
            covenant.to_consensus_bytes().to_hex(),
            "21202220310153563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd"
        );
    }

    #[test]
    fn nested() {
        let covenant = covenant!(xor(
            identity(),
            and(identity(), and(not(identity(),), and(identity(), identity())))
        ));
        assert_eq!(covenant.to_consensus_bytes().to_hex(), "23202120212420212020");
        let h = from_hex("53563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd").unwrap();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(h.as_slice());
        let covenant = covenant!(and(
            or(
                identity(),
                fields_hashed_eq(
                    @fields(@field::commitment, @field::features_metadata),
                    @hash(hash),
                ),
            ),
            field_eq(@field::features_maturity, @uint(42))
        ));
        assert_eq!(
            covenant.to_consensus_bytes().to_hex(),
            "21222032080200090153563b674ba8e5166adb57afa8355bcf2ee759941eef8f8959b802367c2558bd330706062a"
        );
    }

    #[test]
    fn covenant() {
        let bytes = vec![0xba, 0xda, 0x55];
        let covenant = covenant!(field_eq(@field::covenant, @covenant(and(field_eq(@field::features_unique_id, @bytes(bytes), identity())))));
        assert_eq!(covenant.to_consensus_bytes().to_hex(), "330703050a213307070903bada5520");
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

        let decoded = Covenant::consensus_decode(&mut covenant.to_consensus_bytes().as_slice()).unwrap();
        assert_eq!(covenant, decoded);
        unpack_enum!(CovenantArg::TariScript(decoded_script) = decoded.tokens()[2].as_arg().unwrap());
        assert_eq!(script, *decoded_script);
    }
}
