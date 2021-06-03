#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod error;

use crate::error::{InterfaceError, MiningcoreError};
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
        error = MiningcoreError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return false;
    } else {
        native = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }
    let pk = TariPublicKey::from_hex(&native);
    match pk {
        Ok(_pk) => true,
        Err(e) => {
            error = MiningcoreError::from(e).code;
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
        error = MiningcoreError::from(InterfaceError::NullError("hex".to_string())).code;
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
                        error = MiningcoreError::from(InterfaceError::ConversionError("block".to_string())).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        ptr::null()
                    },
                }
            },
            Err(_) => {
                error = MiningcoreError::from(InterfaceError::ConversionError("hex".to_string())).code;
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
        error = MiningcoreError::from(InterfaceError::NullError("hex".to_string())).code;
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
                    error = MiningcoreError::from(InterfaceError::ConversionError("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    0
                },
            }
        },
        Err(_) => {
            error = MiningcoreError::from(InterfaceError::ConversionError("hex".to_string())).code;
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
) -> c_int
{
    let mut error = 0;
    ptr::swap(error_out, &mut error as *mut c_int);
    let block_hex_string;
    let block_hash_string;

    if hex.is_null() {
        error = MiningcoreError::from(InterfaceError::NullError("hex".to_string())).code;
        ptr::swap(error_out, &mut error as *mut c_int);
        return 2;
    } else {
        block_hex_string = CString::from_raw(hex as *mut i8).to_str().unwrap().to_owned();
    }

    if hash.is_null() {
        error = MiningcoreError::from(InterfaceError::NullError("hash".to_string())).code;
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
                    if block.hash().to_hex() == block_hash_string {
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
                            error = MiningcoreError::from(InterfaceError::LowDifficultyError(block_hash_string)).code;
                            ptr::swap(error_out, &mut error as *mut c_int);
                        }
                        result
                    } else {
                        error = MiningcoreError::from(InterfaceError::InvalidHashError(block_hash_string)).code;
                        ptr::swap(error_out, &mut error as *mut c_int);
                        2
                    }
                },
                Err(_) => {
                    error = MiningcoreError::from(InterfaceError::ConversionError("block".to_string())).code;
                    ptr::swap(error_out, &mut error as *mut c_int);
                    2
                },
            }
        },
        Err(_) => {
            error = MiningcoreError::from(InterfaceError::ConversionError("hex".to_string())).code;
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

    const BLOCK_HEX: &str = "7b22686561646572223a7b2276657273696f6e223a312c22686569676874223a36363139372c22707265765f68617368223a2265323734323439643333353737633837643930383239633365656133386539363438366664643230343634623739626232666635633633366330343230346463222c2274696d657374616d70223a313632353437393437382c226f75747075745f6d72223a2262303536303863653033373034393562373834313363376262623264663634383339653963646534663230383432613334326434313261333837393038326261222c2272616e67655f70726f6f665f6d72223a2263363166363533613062313137333237316531316431623566623838396561316365326362326566373432326233316565666133313535656534393632376434222c226f75747075745f6d6d725f73697a65223a3336383733372c226b65726e656c5f6d72223a2264303737656230393134313530376237336336643031323931626566346562353534636637366235363934333835653031663363373330613132376535376434222c226b65726e656c5f6d6d725f73697a65223a3130343038322c22746f74616c5f6b65726e656c5f6f6666736574223a2230303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030303030222c226e6f6e6365223a302c22706f77223a7b22706f775f616c676f223a2253686133222c22706f775f64617461223a5b5d7d7d2c22626f6479223a7b22736f72746564223a66616c73652c22696e70757473223a5b5d2c226f757470757473223a5b7b226665617475726573223a7b22666c616773223a7b2262697473223a317d2c226d61747572697479223a36363235377d2c22636f6d6d69746d656e74223a2231616138333963356466333364376637316131343732313763633138333063626661363537303261663561356364353061656630346361613630613861393330222c2270726f6f66223a22386563393632353063643662633034653566663734386331613237393965626661336330373463303033653466353461353063316232666535363133373930623365316339643036643665393232356364323861366230363634346635653732636364316566613839343339383530663233656238333564373565353162306636303638383838353538353132653931363963666338336166343666363632393638373565356133336163343238616237356637666563333162363333653630306563626262643537343932663866623264353432383139663033643337363164656130306339653838326637313934363839313564353736643662336636633634313231643665626232303438306634383234303738383364643961613134363430316135666465623932363936336465393039636264386237396534306565636266323434613934356131356465303935393935393166373934316230383035333831323639303437346331616636313162396364346263396466303033653533346266656363636631333334616237343938323239656464326638623765636165366238313033653330393939633363656638316331656634343830653463373035346431323936393830653034656661616136346264653536356437303139346561373832373538323063376563653261633533353836386661353733323730373836336138313832373563353633303132643539616665333938613132613732616531343639656361666634346364343837393364633835633339383264303965343365623832363931353331336539343433326534643033336439303834626335343632313134303739383239663230306237333430333431666132313965363931346530373732663361333839373864306366666531633934383362326366363734623233643265343661303832373935363931616266333562343931623766666136343362376239313464616530343031373939346539663764323064613265326665373764636235313030393736653364326235363333393231653966313865336365366535343666663534663038306461646130346230366230333166636630623938373037343530393432323564323738313034636336323135313361303664643533393863336336356133313331343333663263303032323562626166323765346435323632636335376439343537646538336231343637636437376431656262383536636138633239656439623232326561653130663466306338656464643630623631333962333434643263326330313030646331643465616132323534643065306365373766633039636665343636346139303364303764303234363430613465323862373233393666623666333936313236616535383465363561306135336166656637363933623464373439306335353033333430653137313537356433626165323738363931656333383531343062383334336535353737633863336663353666643163376366376263636635346463393062316132306364396434343264353333373037636532616132643363613239653935313334303761623038626232636363336663316430356631333162346264643662633338333937616164326263393935303332653165306533663331326662646366343564643864636362326264336162373338633464363064656464303263313165386437663339643332653531643733303739376161303038623065306135613535376462636638393861646564346639356434306132653539326263303764646364646539356132643130646465313734383665303064227d5d2c226b65726e656c73223a5b7b226665617475726573223a7b2262697473223a317d2c22666565223a302c226c6f636b5f686569676874223a302c22657863657373223a2233616332363532316632646561306232346235343831636131303762613035656164303363303239363239303038633161323264336633623862333563623037222c226578636573735f736967223a7b227075626c69635f6e6f6e6365223a2232366337383865396336353834326238303734626230393663373665633535373332343735616533336139383665643164653131373439633536386238633362222c227369676e6174757265223a2234313366363039323462336331616437343139613234353763666330613536636365343932323665653134356533346538353736666438656566633033313066227d7d5d7d7d";
    const HASH_HEX: &str = "1d88a679d89801cbd0627297071ad0436e1fbe4184eaa49514951d4bfe506a38";
    const NONCE: u64 = 13308013039806880791;

    #[test]
    fn check_difficulty() {
        // Difficulty 25
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let block_hex_ptr2 = inject_nonce(block_hex_ptr, NONCE, error_ptr);
            let result = share_difficulty(block_hex_ptr2, error_ptr);
            assert_eq!(result, 25);
        }
    }

    #[test]
    fn check_invalid_share() {
        // Difficulty 25
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 50;
            let stratum_difficulty = 30;
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
        // Difficulty 25
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 30;
            let stratum_difficulty = 20;
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
        // Difficulty 25
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let block_hex = CString::new(BLOCK_HEX).unwrap();
            let hash_hex = CString::new(HASH_HEX).unwrap();
            let block_hex_ptr: *const c_char = CString::into_raw(block_hex) as *const c_char;
            let hash_hex_ptr: *const c_char = CString::into_raw(hash_hex) as *const c_char;
            let template_difficulty = 20;
            let stratum_difficulty = 5;
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
            assert_eq!(success, true);
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
            assert_eq!(success, false);
            assert_ne!(error, 0);
        }
        unsafe {
            let mut error = -1;
            let error_ptr = &mut error as *mut c_int;
            let test_pk = CString::new("5fe83bf62521629ca185098ac24c7b02b184c2e0a2b01455f3a5957d5d").unwrap();
            let test_pk_ptr: *const c_char = CString::into_raw(test_pk) as *const c_char;
            let success = public_key_hex_validate(test_pk_ptr, error_ptr);
            assert_eq!(success, false);
            assert_ne!(error, 0);
        }
    }
}
