// Copyright 2021. The Tari Project
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
//
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod error;

use crate::error::{InterfaceError, StratumTranscoderError};
use core::ptr;
use libc::{c_char, c_int, c_ulonglong};
use std::ffi::CString;
use tari_core::{
    blocks::Block,
    crypto::tari_utilities::{message_format::MessageFormat, Hashable},
    proof_of_work::{sha3_difficulty, Difficulty},
};
use tari_crypto::tari_utilities::hex::Hex;
pub type TariPublicKey = tari_comms::types::CommsPublicKey;

/// Validates a hex string is convertible into a TariPublicKey
///
/// ## Arguments
/// `hex` - The hex formatted cstring to be validated
///
/// ## Returns
/// `bool` - Returns true/false
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn public_key_hex_validate(hex: *const c_char, error_out: *mut c_int) -> bool {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let native;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }
    let pk = TariPublicKey::from_hex(&native);
    match pk {
        Ok(_pk) => true,
        Err(e) => {
            error = StratumTranscoderError::from(e).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            false
        },
    }
}

/// Injects a nonce into a blocktemplate
///
/// ## Arguments
/// `hex` - The hex formatted cstring
/// `nonce` - The nonce to be injected
///
/// ## Returns
/// `c_char` - The updated hex formatted cstring or null on error
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn inject_nonce(hex: *const c_char, nonce: c_ulonglong, error_out: *mut c_int) -> *const c_char {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let native;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        ptr::null()
    } else {
        native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
        let block_hex = hex::decode(native);
        match block_hex {
            Ok(block_hex) => {
                let block: Result<Block, serde_json::Error> =
                    serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
                match block {
                    Ok(mut block) => {
                        block.header.nonce = nonce;
                        let block_json = block.to_json().unwrap();
                        let block_hex = hex::encode(block_json);
                        let result = CString::new(block_hex).unwrap();
                        CString::into_raw(result)
                    },
                    Err(_) => {
                        error = StratumTranscoderError::from(InterfaceError::ConversionError("block".to_string())).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        ptr::null()
                    },
                }
            },
            Err(_) => {
                error = StratumTranscoderError::from(InterfaceError::ConversionError("hex".to_string())).code;
                ptr::swap(error_out, &mut error as *mut c_int);
                ptr::null()
            },
        }
    }
}

/// Returns the difficulty of a share
///
/// ## Arguments
/// `hex` - The hex formatted cstring to be validated
///
/// ## Returns
/// `c_ulonglong` - Difficulty, 0 on error
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn share_difficulty(hex: *const c_char, error_out: *mut c_int) -> c_ulonglong {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let block_hex_string;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 0;
    } else {
        block_hex_string = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }

    let block_hex = hex::decode(block_hex_string);
    match block_hex {
        Ok(block_hex) => {
            let block: Result<Block, serde_json::Error> =
                serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
            match block {
                Ok(block) => {
                    let difficulty = sha3_difficulty(&block.header);
                    difficulty.as_u64()
                },
                Err(_) => {
                    error = StratumTranscoderError::from(InterfaceError::ConversionError("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    0
                },
            }
        },
        Err(_) => {
            error = StratumTranscoderError::from(InterfaceError::ConversionError("hex".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            0
        },
    }
}

/// Validates a share submission
///
/// ## Arguments
/// `hex` - The hex representation of the share to be validated
/// `hash` - The hash of the share to be validated
/// `nonce` - The nonce for the share to be validated
/// `stratum_difficulty` - The stratum difficulty to be checked against (meeting this means that the share is valid for
/// payout) `template_difficulty` - The difficulty to be checked against (meeting this means the share is also a block
/// to be submitted to the chain)
///
/// ## Returns
/// `c_uint` - Returns one of the following:
///             0: Valid Block
///             1: Valid Share
///             2: Invalid Share
/// `error_out` - Error code returned, 0 means no error
///
/// # Safety
/// None
#[no_mangle]
pub unsafe extern "C" fn share_validate(
    hex: *const c_char,
    hash: *const c_char,
    stratum_difficulty: c_ulonglong,
    template_difficulty: c_ulonglong,
    error_out: *mut c_int,
) -> c_int {
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let block_hex_string;
    let block_hash_string;

    if hex.is_null() {
        error = StratumTranscoderError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    } else {
        block_hex_string = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }

    if hash.is_null() {
        error = StratumTranscoderError::from(InterfaceError::NullError("hash".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    } else {
        block_hash_string = CString::from_raw(hash as *mut i8).to_str().unwrap().to_owned();
    }

    let block_hex = hex::decode(block_hex_string);
    match block_hex {
        Ok(block_hex) => {
            let block: Result<Block, serde_json::Error> =
                serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
            match block {
                Ok(block) => {
                    if block.header.hash().to_hex() == block_hash_string {
                        // Hash submitted by miner is the same hash produced for the nonce submitted by miner
                        let mut result = 2;
                        let difficulty = sha3_difficulty(&block.header);
                        if difficulty >= Difficulty::from(template_difficulty) {
                            // Valid block
                            result = 0;
                        } else if difficulty >= Difficulty::from(stratum_difficulty) {
                            // Valid share
                            result = 1;
                        } else {
                            // Difficulty not reached
                            error = StratumTranscoderError::from(InterfaceError::LowDifficultyError(block_hash_string))
                                .code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                        }
                        result
                    } else {
                        error = StratumTranscoderError::from(InterfaceError::InvalidHashError(block_hash_string)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        2
                    }
                },
                Err(_) => {
                    error = StratumTranscoderError::from(InterfaceError::ConversionError("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    2
                },
            }
        },
        Err(_) => {
            error = StratumTranscoderError::from(InterfaceError::ConversionError("hex".to_string())).code;
            ptr::swap(error_out, &mut error as *mut c_int);
            2
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{inject_nonce, public_key_hex_validate, share_difficulty, share_validate};
    use libc::{c_char, c_int};
    use std::{ffi::CString, str};

    const BLOCK_HEX: &str = "7b22686561646572223a7b2276657273696f6e223a312c22686569676874223a343333382c22707265765f68617368223a2237663665626130376432373964366464316566656263376564346431386163396436666564663366613536303131363835636361326465336562656232633266222c2274696d657374616d70223a313632363138353739372c226f75747075745f6d72223a2237336230306466393130353263383831343061393765613831343138396239356335313634303662633434323238666562393262326563333238386534366564222c227769746e6573735f6d72223a2236326665643734633863633531633032363338356638626434663330326638306263353034393635656363363930393033646565623765613836303331376531222c226f75747075745f6d6d725f73697a65223a3130303439382c226b65726e656c5f6d72223a2263653233656430623561663938323236653936353533636631616539646538346230333432363665316164366435623231383531356431306663613930393132222c226b65726e656c5f6d6d725f73697a65223a32303438332c22696e7075745f6d72223a2232363134366135343335656631356538636637646333333534636237323638313337653862653231313739346539336430343535313537366336353631353635222c22746f74616c5f6b65726e656c5f6f6666736574223a2230303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030222c22746f74616c5f7363726970745f6f6666736574223a2230303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030222c226e6f6e6365223a302c22706f77223a7b22706f775f616c676f223a2253686133222c22706f775f64617461223a5b5d7d7d2c22626f6479223a7b22736f72746564223a66616c73652c22696e70757473223a5b5d2c226f757470757473223a5b7b226665617475726573223a7b22666c616773223a7b2262697473223a317d2c226d61747572697479223a343334347d2c22636f6d6d69746d656e74223a2264656138316332663165353461336465323035363764656665623335613730643530666165343730313438626532666335316134303330666335653764373036222c2270726f6f66223a22336339643164353032653165313637656132366336383538373931666138653836373833303062616334656264616635386261333435376566333735666432393138663566393034306638623534616363313264373031383463336234333362643236663161316234356335313739653233366535633434636665613336333362323764633031356663643266306139333861393864326633363164623466386231656335333466306262636135626661373731663838373430313764323132356331373839333437316235633462313665626262346165616137313434636666653332326361613438613436326436626462343661373534613132616336333532333365656530353463366337343766623132353436636664646561323562346365336230643364333332653763396363376137646365646334383662306533313866333132376233313735306336346634653533666339393239366666383365306332633232636235333464396262613533316562393364626433613034386635626431366563643239613939636630623436386133616332666233393439363735303964623033316332666363616636613831653030303766353330356563623730613638653537393166356462396237376162313634393434626430396665356439393564666337633933663865316435306639643362616639363330653164303737643565323237356436343834323833656461313163373139393330343637363037643761306631633561613139386463343331633736643732653436303736663030313738633466363535313432366161653437633263656263386165373932393966313732303163626261313837396565616238346637636430303737353639643864323933306437623464363261303337313765613731376632386363616366633438636135643665383037313239306234386132343736616430663562623039633762303930376231616533623133653262653136643531613465303832386364393366353734336534323939303835613936663032356338656633383436623430633634386563633733666431643065633535376166313632376362626538626639643430333232303833336138353633343337316334666639663636363663313239303436616263323939633633643064313532626437306464303336306265396339383961396133643930653639613031366164633064663937373664323661343434303237633033623263303639643438613031383762313365643236386430366530313961363733663163643636613436623838333335663663313562363566663232383737346334383536653564323466336465363633633636333739663639376162323039323537326265663434346363306361366433396562383732616538363765373536356131626539613731396231613130613833363937656133333666333438613033373864613365373036303534356434323233396138313438303632303564306466376138663961613438633834383362353432663862303564346330626235333039363534373032306137663366316362333137633733346532373866303232396234396263333635666539373935393730613662666163326462626537633337616436666337373266323038613463333637653634333030663963623136363332643034346333626436386237613939383830663533336630346465613030633761343637303035613261316432313766343261323935623264393565646664393632346463636535343432653763393039663661333834363036346466643765373538303066222c22736372697074223a223733222c2273656e6465725f6f66667365745f7075626c69635f6b6579223a2264616133376465323133323038636462323237623431666435313830643530306130643138356462346565353461646666643033386436346233386136353764222c226d657461646174615f7369676e6174757265223a7b227075626c69635f6e6f6e6365223a2261306565623636383862613363313331616565343538363435396662336533323463303537316535656639643937316462303461313331643061636435343331222c2275223a2262383633666563386336396361313136393166383363656165633531653839393833613235363334666563306438383035326232363066383862313835353032222c2276223a2264396535323238346662393536666665343837636238376538353666373837343939356366616162393034373264376432616537616539623431373537393032227d7d5d2c226b65726e656c73223a5b7b226665617475726573223a7b2262697473223a317d2c22666565223a302c226c6f636b5f686569676874223a302c22657863657373223a2263366263386263643162623836353964666664356537363634653263363265646333383639333566396230633033333130353265383836623235623264373465222c226578636573735f736967223a7b227075626c69635f6e6f6e6365223a2236326264336539663631643362633031323738386130373134633461666134353332383136663562616664613138303465623963643333616536356538323465222c227369676e6174757265223a2234643662323666383433623837623737393734343233613764656563303365663933653930326563633131393734303837646264643234333362643936363061227d7d5d7d7d";
    const HASH_HEX: &str = "3a9ea717ca7b2598d900e2ef98c270ac98ce993bce8a9e058929967ba37fbc6b";
    const NONCE: u64 = 15810795621223647638;

    #[test]
    fn check_difficulty() {
        // Difficulty 20025
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_difficulty(block_hex_ptr2, error_ptr);
            assert_eq!(result, 20025);
        }
    }

    #[test]
    fn check_invalid_share() {
        // Difficulty 20025
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 30000;
            let stratum_difficulty = 22200;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 2);
            assert_eq!(error, 4);
        }
    }

    #[test]
    fn check_valid_share() {
        // Difficulty 20025
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 30000;
            let stratum_difficulty = 20000;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 1);
            assert_eq!(error, 0);
        }
    }

    #[test]
    fn check_valid_block() {
        // Difficulty 20025
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 20000;
            let stratum_difficulty = 15000;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_validate(
                block_hex_ptr2,
                hash_hex_ptr,
                stratum_difficulty,
                template_difficulty,
                error_ptr,
            );
            assert_eq!(result, 0);
            assert_eq!(error, 0);
        }
    }

    #[test]
    fn check_valid_address() {
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5ce83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5df94126").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert_eq!(error, 0);
            assert!(success);
        }
    }

    #[test]
    fn check_invalid_address() {
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5fe83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5df94126").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert!(!success);
            assert_ne!(error, 0);
        }
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5fe83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5d").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert!(!success);
            assert_ne!(error, 0);
        }
    }
}
