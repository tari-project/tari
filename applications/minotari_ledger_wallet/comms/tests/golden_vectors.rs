// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Golden wire vectors: the exact bytes every instruction puts on the wire, and the exact bytes every reply is
//! parsed from.
//!
//! # Why these exist
//!
//! Every APDU layout used to be written twice - a host encoder in `accessor_methods` and a device decoder in the
//! Ledger application's handlers - with nothing tying the two together. The shared wire-format codec in
//! `minotari_ledger_wallet_common` replaces both, and that refactor is required to be *byte frozen*: a device in the
//! field running the previous application must keep understanding the host, and vice versa. These vectors were
//! captured from the hand rolled encoders **before** the codec existed, and they must pass unchanged at every step of
//! the migration and afterwards.
//!
//! A change to any constant below is therefore a wire format change, which needs its own spec and its own
//! application version bump. It is never a test fix.
//!
//! # What is covered
//!
//! - **Requests**, end to end through the public accessor methods the console wallet calls, so that the vector pins the
//!   shipped behaviour rather than a helper the shipped code might stop using. The whole serialised APDU is compared:
//!   class, instruction, `p1`, `p2`, length and data.
//! - **Replies**, by feeding a frozen reply to the same accessor and checking what it parsed out of it. That pins the
//!   offsets the host reads from; the device side is pinned by the codec's own encoders being checked against the same
//!   constants.
//!
//! The inputs are fixed and deliberately distinct from one another - every scalar has its own fill byte - so that
//! two fields swapping places changes the vector rather than cancelling out.
//!
//! # One table, one test
//!
//! The accessor methods go through `verify_ledger_application`, whose cached result is process wide, and through
//! the process wide registered transport. A second test in this binary that drove an accessor would race this one
//! over both, so every request vector is checked inside a single `#[test]`. Tests that only look at the constants
//! themselves touch neither, and are free to live alongside it.

#![cfg(feature = "test_transport")]

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use ledger_transport::{APDUAnswer, APDUCommand};
use minotari_ledger_wallet_common::{
    codec::{
        ComAndPubSigReply,
        Decode,
        Encode,
        EphemeralNonceReply,
        GenerateEphemeralNonceRequest,
        GetAppNameRequest,
        GetDHSharedSecretRequest,
        GetOneSidedMetadataSignatureRequest,
        GetPublicKeyRequest,
        GetPublicSpendKeyRequest,
        GetRawSchnorrSignatureLegacyNonceRequest,
        GetRawSchnorrSignatureRequest,
        GetScriptSchnorrSignatureRequest,
        GetScriptSignatureDerivedRequest,
        GetScriptSignatureManagedRequest,
        GetVersionRequest,
        GetViewKeyRequest,
        KeyReply,
        SchnorrReply,
        ScriptSignatureCommon,
        TextReply,
    },
    common_types::{Instruction, LedgerKeyBranch},
};
use minotari_ledger_wallet_comms::{
    accessor_methods::{
        ScriptSignatureKey,
        ledger_generate_ephemeral_nonce,
        ledger_get_app_name,
        ledger_get_dh_shared_secret,
        ledger_get_one_sided_metadata_signature,
        ledger_get_public_key,
        ledger_get_public_spend_key,
        ledger_get_raw_schnorr_signature,
        ledger_get_raw_schnorr_signature_legacy_nonce,
        ledger_get_script_offset,
        ledger_get_script_schnorr_signature,
        ledger_get_script_signature,
        ledger_get_version,
        ledger_get_view_key,
        verify_ledger_application,
    },
    error::LedgerDeviceError,
    ledger_wallet::{EXPECTED_NAME, LedgerTransport, MIN_LEDGER_APP_VERSION, register_transport},
};
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::{CompressedCommitment, CompressedPublicKey, PrivateKey},
};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_script::CheckSigSchnorrSignature;
use tari_utilities::{ByteArray, hex::Hex};

// ------------------------------------------------------------------------------------------------------------------
// Fixed inputs
// ------------------------------------------------------------------------------------------------------------------

/// Little endian `01 02 03 04 05 06 07 08` on the wire, so the account is recognisable in every vector.
const ACCOUNT: u64 = 0x0807_0605_0403_0201;
/// Little endian `18 17 16 15 14 13 12 11`.
const INDEX: u64 = 0x1112_1314_1516_1718;
/// Little endian `21 22 23 24 25 26 27 28`.
const NONCE_INDEX: u64 = 0x2827_2625_2423_2221;
/// Little endian `31 32 33 34 35 36 37 38`.
const NONCE_HANDLE: u64 = 0x3837_3635_3433_3231;
/// Little endian `41 42 43 44 45 46 47 48`.
const SENDER_OFFSET_KEY_INDEX: u64 = 0x4847_4645_4443_4241;
/// Little endian `51 52 53 54 55 56 57 58`.
const BASE_INDEX: u64 = 0x5857_5655_5453_5251;
/// 1.234567 T, little endian `87 d6 12 00 00 00 00 00`.
const VALUE: u64 = 1_234_567;
const TXO_VERSION: u8 = 1;
const NETWORK: Network = Network::Esmeralda;

/// A canonical scalar whose bytes are all `fill` bar the most significant, which is `0x01` so that it is below the
/// group order whatever `fill` is.
fn scalar(fill: u8) -> PrivateKey {
    let mut bytes = [fill; 32];
    *bytes.last_mut().expect("32 bytes") = 0x01;
    PrivateKey::from_canonical_bytes(&bytes).expect("canonical by construction")
}

/// A valid compressed point, deterministic in `fill`.
fn point(fill: u8) -> CompressedPublicKey {
    CompressedPublicKey::from_secret_key(&scalar(fill))
}

fn ristretto_point(fill: u8) -> RistrettoPublicKey {
    RistrettoPublicKey::from_secret_key(&scalar(fill))
}

fn commitment(fill: u8) -> CompressedCommitment {
    CompressedCommitment::from_canonical_bytes(point(fill).as_bytes()).expect("a valid point is a valid commitment")
}

/// A dual address with a short payment id, so that the variable length address field is exercised with something
/// other than its minimum size.
fn receiver_address() -> TariAddress {
    TariAddress::new_dual_address(
        point(0xd1),
        point(0xd2),
        NETWORK,
        TariAddressFeatures::create_one_sided_only(),
        Some(vec![0xee, 0xef, 0xf0]),
    )
    .expect("valid address")
}

// ------------------------------------------------------------------------------------------------------------------
// Golden request vectors: the serialised APDU, `cla | ins | p1 | p2 | lc | data`.
// ------------------------------------------------------------------------------------------------------------------

/// `GetVersion` and `GetAppName` carry a random account the device ignores; it is zeroed before comparison.
const GET_VERSION_REQUEST: &str = concat!(
    "80",               // cla
    "01",               // ins
    "00",               // p1
    "00",               // p2
    "09",               // lc
    "0000000000000000", // account (random, zeroed)
    "00",               // unused
);
const GET_APP_NAME_REQUEST: &str = concat!(
    "80",               // cla
    "02",               // ins
    "00",               // p1
    "00",               // p2
    "09",               // lc
    "0000000000000000", // account (random, zeroed)
    "00",               // unused
);
const GET_PUBLIC_SPEND_KEY_REQUEST: &str = concat!(
    "80",               // cla
    "03",               // ins
    "00",               // p1
    "00",               // p2
    "08",               // lc
    "0102030405060708", // account
);
const GET_PUBLIC_KEY_REQUEST: &str = concat!(
    "80",               // cla
    "04",               // ins
    "00",               // p1
    "00",               // p2
    "18",               // lc
    "0102030405060708", // account
    "1817161514131211", // index
    "0800000000000000", // branch (Random, widened to u64)
);
const GET_VIEW_KEY_REQUEST: &str = concat!(
    "80",               // cla
    "07",               // ins
    "00",               // p1
    "00",               // p2
    "08",               // lc
    "0102030405060708", // account
);
const GET_DH_SHARED_SECRET_REQUEST: &str = concat!(
    "80",                                                               // cla
    "08",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "38",                                                               // lc
    "0102030405060708",                                                 // account
    "1817161514131211",                                                 // index
    "0600000000000000",                                                 // branch (OneSidedSenderOffset)
    "0064187361d418062ac29dc2de0f3b24a32dfdccdd5086e42fa6788d1c020a40", // public_key
);
const GET_SCRIPT_SIGNATURE_MANAGED_REQUEST: &str = concat!(
    "80",                                                               // cla
    "12",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "a8",                                                               // lc
    "0102030405060708",                                                 // account
    "2600000000000000",                                                 // network (Esmeralda, widened to u64)
    "0100000000000000",                                                 // txi_version (widened to u64)
    "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b201", // value
    "b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b301", // commitment_private_key
    "1ea292d5e5f53230eb6afba515ad27e9b5927342e7a2c7d69d3ec3cfff8dbd39", // commitment
    "b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5", // message
    "0900000000000000",                                                 // branch (PreMine)
    "1817161514131211",                                                 // index
);
const GET_SCRIPT_SIGNATURE_DERIVED_REQUEST: &str = concat!(
    "80",                                                               // cla
    "05",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "b8",                                                               // lc
    "0102030405060708",                                                 // account
    "2600000000000000",                                                 // network (Esmeralda, widened to u64)
    "0100000000000000",                                                 // txi_version (widened to u64)
    "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b201", // value
    "b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b3b301", // commitment_private_key
    "1ea292d5e5f53230eb6afba515ad27e9b5927342e7a2c7d69d3ec3cfff8dbd39", // commitment
    "b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5b5", // message
    "b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b601", // blinding_factor
);
const GET_SCRIPT_SCHNORR_SIGNATURE_REQUEST: &str = concat!(
    "80",                                                               // cla
    "10",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "38",                                                               // lc
    "0102030405060708",                                                 // account
    "1817161514131211",                                                 // index
    "0600000000000000",                                                 // branch (OneSidedSenderOffset)
    "b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7", // message
);
const GET_ONE_SIDED_METADATA_SIGNATURE_REQUEST: &str = concat!(
    "80",                                                               // cla
    "11",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "b0",                                                               // lc
    "0102030405060708",                                                 // account
    "2600000000000000",                                                 // network (Esmeralda, widened to u64)
    "0100000000000000",                                                 // txo_version (widened to u64)
    "4142434445464748",                                                 // sender_offset_key_index
    "87d6120000000000",                                                 // value
    "b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b8b801", // commitment_mask
    "4600",                                                             // address_size (70, u16)
    "2605b676b11050b58e4adbd76040817cc822865ed00073fad2b0d53d72e074ce", // receiver_address (dual, 3 byte payment id)
    "66331ea3494b3fc582744b92cea35e0f230322bc68a88851713d5ba8a8ea8594",
    "4711eeeff07d",
    "b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9", // message
);
const GENERATE_EPHEMERAL_NONCE_REQUEST: &str = concat!(
    "80",               // cla
    "13",               // ins
    "00",               // p1
    "00",               // p2
    "08",               // lc
    "0102030405060708", // account
);
const GET_RAW_SCHNORR_SIGNATURE_REQUEST: &str = concat!(
    "80",                                                               // cla
    "09",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "60",                                                               // lc
    "0102030405060708",                                                 // account
    "1817161514131211",                                                 // index
    "0800000000000000",                                                 // branch (Random)
    "3132333435363738",                                                 // nonce_handle
    "babababababababababababababababababababababababababababababababa", // challenge
    "babababababababababababababababababababababababababababababababa",
);
const GET_RAW_SCHNORR_SIGNATURE_LEGACY_NONCE_REQUEST: &str = concat!(
    "80",                                                               // cla
    "14",                                                               // ins
    "00",                                                               // p1
    "00",                                                               // p2
    "68",                                                               // lc
    "0102030405060708",                                                 // account
    "1817161514131211",                                                 // key_index
    "0900000000000000",                                                 // key_branch (PreMine)
    "2122232425262728",                                                 // nonce_index
    "0800000000000000",                                                 // nonce_branch (Random)
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", // challenge
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
);
/// Header, partial sum, two indexed script keys, two derived script keys.
const GET_SCRIPT_OFFSET_REQUEST: [&str; 6] = [
    // chunk 0: header
    concat!(
        "80",               // cla
        "06",               // ins
        "00",               // p1
        "01",               // p2
        "20",               // lc
        "0102030405060708", // account
        "0300000000000000", // sender_offset_count
        "0200000000000000", // script_index_count
        "0200000000000000", // derived_script_key_count
    ),
    // chunk 1: partial sum
    concat!(
        "80",                                                               // cla
        "06",                                                               // ins
        "01",                                                               // p1
        "01",                                                               // p2
        "20",                                                               // lc
        "bcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbcbc01", // partial_script_key_sum
    ),
    // chunk 2: indexed script key
    concat!(
        "80",               // cla
        "06",               // ins
        "02",               // p1
        "01",               // p2
        "10",               // lc
        "0900000000000000", // branch (PreMine)
        "1817161514131211", // index
    ),
    // chunk 3: indexed script key
    concat!(
        "80",               // cla
        "06",               // ins
        "03",               // p1
        "01",               // p2
        "10",               // lc
        "0900000000000000", // branch (PreMine)
        "2122232425262728", // index
    ),
    // chunk 4: derived script key
    concat!(
        "80",                                                               // cla
        "06",                                                               // ins
        "04",                                                               // p1
        "01",                                                               // p2
        "20",                                                               // lc
        "bdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbdbd01", // blinding_factor
    ),
    // chunk 5: derived script key
    concat!(
        "80",                                                               // cla
        "06",                                                               // ins
        "05",                                                               // p1
        "00",                                                               // p2
        "20",                                                               // lc
        "bebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebe01", // blinding_factor (p2 = 0: last chunk)
    ),
];

// ------------------------------------------------------------------------------------------------------------------
// Golden reply vectors: the reply data, without the status word.
// ------------------------------------------------------------------------------------------------------------------

/// `version(1) | key(32)`: `GetPublicSpendKey`, `GetPublicKey`, `GetDHSharedSecret`.
const KEY_REPLY: &str = "021a71c7c14d1e286cc5c446387181f53653801cafa67dad6668d00b2bea99b10a";
/// `version(1) | scalar(32)`: `GetViewKey`.
const VIEW_KEY_REPLY: &str = "02a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a201";
/// `version(1) | public_nonce(32) | signature(32)`: all three Schnorr instructions.
const SCHNORR_REPLY: &str = concat!(
    "02",
    "3a914c54b86a03c4f075d1b9b572ac067966b9695947fdfe6cfd70a298d8d05b",
    "a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a401",
);
/// `version(1) | ephemeral_commitment(32) | ephemeral_pubkey(32) | u_a(32) | u_x(32) | u_y(32)`: both script
/// signature instructions and `GetOneSidedMetadataSignature`.
const COM_AND_PUB_SIG_REPLY: &str = concat!(
    "02",
    "c837ba70f0182d6b4f1b0f70d60739274598e38ff969ff0aab16e3f248a91f7e",
    "103e936b79ab4c2784c1e57d7fc706b5ae43af1f4576d8b93102bb31463d6659",
    "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c301",
    "c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c4c401",
    "c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c501",
);
/// `version(1) | handle(8) | public_nonce(32)`.
const EPHEMERAL_NONCE_REPLY: &str = concat!(
    "02",
    "3132333435363738",
    "ae10d22298446feed478d98b2d4e5d9c3701a0031c5ea0b23577667374baf52a",
);
/// `version(1) | script_offset(32) | base_index(8)`.
const SCRIPT_OFFSET_REPLY: &str = concat!(
    "02",
    "a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a6a601",
    "5152535455565758",
);

// ------------------------------------------------------------------------------------------------------------------
// The stub device
// ------------------------------------------------------------------------------------------------------------------

const STATUS_OK: u16 = 0x9000;
const RESPONSE_VERSION: u8 = 2;

/// Records every APDU it is sent and answers with whatever reply was queued for it.
///
/// With nothing queued it behaves like a well formed device for the five exchanges `verify()` makes, which need
/// real keys and signatures that verify - so that the process wide verification can complete once, up front,
/// without its exchanges landing in the recorded vectors.
struct GoldenDevice {
    sent: Mutex<Vec<APDUCommand<Vec<u8>>>>,
    replies: Mutex<VecDeque<Vec<u8>>>,
    keys: Mutex<HashMap<Vec<u8>, RistrettoSecretKey>>,
}

impl GoldenDevice {
    fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            replies: Mutex::new(VecDeque::new()),
            keys: Mutex::new(HashMap::new()),
        }
    }

    /// Queue the replies for the next exchanges, and forget everything sent so far.
    fn expect(&self, replies: Vec<Vec<u8>>) {
        self.sent.lock().unwrap().clear();
        *self.replies.lock().unwrap() = replies.into();
    }

    /// Everything sent since the last [`Self::expect`], serialised. Also checks every queued reply was used.
    fn take_sent(&self) -> Vec<String> {
        assert!(
            self.replies.lock().unwrap().is_empty(),
            "the accessor made fewer exchanges than replies were queued for it"
        );
        self.sent
            .lock()
            .unwrap()
            .drain(..)
            .map(|command| command.serialize().to_hex())
            .collect()
    }

    fn key_for(&self, key_id: &[u8]) -> RistrettoSecretKey {
        self.keys
            .lock()
            .unwrap()
            .entry(key_id.to_vec())
            .or_insert_with(|| RistrettoSecretKey::random(&mut rand::rng()))
            .clone()
    }

    fn verification_reply(&self, command: &APDUCommand<Vec<u8>>) -> Vec<u8> {
        match Instruction::from_byte(command.ins) {
            Some(Instruction::GetAppName) => EXPECTED_NAME.as_bytes().to_vec(),
            Some(Instruction::GetVersion) => MIN_LEDGER_APP_VERSION.as_bytes().to_vec(),
            Some(Instruction::GetPublicKey) => {
                let secret = self.key_for(&command.data);
                let mut data = vec![RESPONSE_VERSION];
                data.extend_from_slice(RistrettoPublicKey::from_secret_key(&secret).as_bytes());
                data
            },
            Some(Instruction::GetScriptSchnorrSignature) => {
                let secret = self.key_for(command.data.get(..24).expect("verification sends 56 bytes"));
                let message = command.data.get(24..56).expect("verification sends 56 bytes");
                let signature = CheckSigSchnorrSignature::sign_with_nonce_and_message(
                    &secret,
                    RistrettoSecretKey::random(&mut rand::rng()),
                    message,
                )
                .unwrap();
                let mut data = vec![RESPONSE_VERSION];
                data.extend_from_slice(signature.get_public_nonce().as_bytes());
                data.extend_from_slice(signature.get_signature().as_bytes());
                data
            },
            other => panic!("nothing was queued for {other:?}"),
        }
    }
}

impl LedgerTransport for GoldenDevice {
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        self.sent.lock().unwrap().push(command.clone());
        let queued = self.replies.lock().unwrap().pop_front();
        let mut data = match queued {
            Some(reply) => reply,
            None => self.verification_reply(command),
        };
        data.extend_from_slice(&STATUS_OK.to_be_bytes());
        APDUAnswer::from_answer(data).map_err(|e| LedgerDeviceError::TransportExchange(e.to_string()))
    }
}

fn unhex(hex: &str) -> Vec<u8> {
    Vec::<u8>::from_hex(hex).expect("golden vectors are valid hex")
}

/// Zero the account of a `GetVersion` or `GetAppName` request: the accessor draws it at random, and the device
/// never reads it.
fn zero_random_account(serialised: &str) -> String {
    let mut bytes = unhex(serialised);
    // cla | ins | p1 | p2 | lc, then the account.
    for byte in bytes.iter_mut().skip(5).take(8) {
        *byte = 0;
    }
    bytes.to_hex()
}

fn assert_request(name: &str, sent: Vec<String>, golden: &[&str]) {
    assert_eq!(
        sent,
        golden.iter().map(|g| (*g).to_string()).collect::<Vec<_>>(),
        "{name}: the bytes on the wire moved. That is a wire format change, not a test fix - see the module docs"
    );
}

// ------------------------------------------------------------------------------------------------------------------
// The table
// ------------------------------------------------------------------------------------------------------------------

#[test]
#[allow(clippy::too_many_lines)]
fn every_instruction_is_byte_identical_to_its_golden_vector() {
    let device = Arc::new(GoldenDevice::new());
    register_transport(device.clone());
    verify_ledger_application().expect("the stub device passes verification");

    // --- GetVersion
    device.expect(vec![MIN_LEDGER_APP_VERSION.as_bytes().to_vec()]);
    assert_eq!(ledger_get_version().unwrap(), MIN_LEDGER_APP_VERSION);
    let sent = device.take_sent().iter().map(|s| zero_random_account(s)).collect();
    assert_request("GetVersion", sent, &[GET_VERSION_REQUEST]);

    // --- GetAppName
    device.expect(vec![EXPECTED_NAME.as_bytes().to_vec()]);
    assert_eq!(ledger_get_app_name().unwrap(), EXPECTED_NAME);
    let sent = device.take_sent().iter().map(|s| zero_random_account(s)).collect();
    assert_request("GetAppName", sent, &[GET_APP_NAME_REQUEST]);

    // --- GetPublicSpendKey
    device.expect(vec![unhex(KEY_REPLY)]);
    let key = ledger_get_public_spend_key(ACCOUNT).unwrap();
    assert_eq!(key, point(0xa1));
    assert_request("GetPublicSpendKey", device.take_sent(), &[GET_PUBLIC_SPEND_KEY_REQUEST]);

    // --- GetPublicKey
    device.expect(vec![unhex(KEY_REPLY)]);
    let key = ledger_get_public_key(ACCOUNT, INDEX, LedgerKeyBranch::Random).unwrap();
    assert_eq!(key, ristretto_point(0xa1));
    assert_request("GetPublicKey", device.take_sent(), &[GET_PUBLIC_KEY_REQUEST]);

    // --- GetViewKey
    device.expect(vec![unhex(VIEW_KEY_REPLY)]);
    let key = ledger_get_view_key(ACCOUNT).unwrap();
    assert_eq!(key, scalar(0xa2));
    assert_request("GetViewKey", device.take_sent(), &[GET_VIEW_KEY_REQUEST]);

    // --- GetDHSharedSecret
    device.expect(vec![unhex(KEY_REPLY)]);
    let secret =
        ledger_get_dh_shared_secret(ACCOUNT, INDEX, LedgerKeyBranch::OneSidedSenderOffset, &point(0xb1)).unwrap();
    assert_eq!(secret, point(0xa1));
    assert_request("GetDHSharedSecret", device.take_sent(), &[GET_DH_SHARED_SECRET_REQUEST]);

    // --- GetScriptSignatureManaged
    let expected_com_and_pub = |signature: &tari_common_types::types::ComAndPubSignature| {
        assert_eq!(signature.ephemeral_commitment(), &commitment(0xc1));
        assert_eq!(signature.ephemeral_pubkey(), &point(0xc2));
        assert_eq!(signature.u_a(), &scalar(0xc3));
        assert_eq!(signature.u_x(), &scalar(0xc4));
        assert_eq!(signature.u_y(), &scalar(0xc5));
    };
    device.expect(vec![unhex(COM_AND_PUB_SIG_REPLY)]);
    let signature = ledger_get_script_signature(
        ACCOUNT,
        NETWORK,
        TXO_VERSION,
        &ScriptSignatureKey::Managed {
            branch: LedgerKeyBranch::PreMine,
            index: INDEX,
        },
        &scalar(0xb2),
        &scalar(0xb3),
        &commitment(0xb4),
        [0xb5; 32],
    )
    .unwrap();
    expected_com_and_pub(&signature);
    assert_request("GetScriptSignatureManaged", device.take_sent(), &[
        GET_SCRIPT_SIGNATURE_MANAGED_REQUEST,
    ]);

    // --- GetScriptSignatureDerived
    device.expect(vec![unhex(COM_AND_PUB_SIG_REPLY)]);
    let signature = ledger_get_script_signature(
        ACCOUNT,
        NETWORK,
        TXO_VERSION,
        &ScriptSignatureKey::Derived {
            branch_key: scalar(0xb6),
        },
        &scalar(0xb2),
        &scalar(0xb3),
        &commitment(0xb4),
        [0xb5; 32],
    )
    .unwrap();
    expected_com_and_pub(&signature);
    assert_request("GetScriptSignatureDerived", device.take_sent(), &[
        GET_SCRIPT_SIGNATURE_DERIVED_REQUEST,
    ]);

    // --- GetScriptSchnorrSignature
    device.expect(vec![unhex(SCHNORR_REPLY)]);
    let signature =
        ledger_get_script_schnorr_signature(ACCOUNT, INDEX, LedgerKeyBranch::OneSidedSenderOffset, &[0xb7; 32])
            .unwrap();
    assert_eq!(signature.get_compressed_public_nonce(), &point(0xa3));
    assert_eq!(signature.get_signature(), &scalar(0xa4));
    assert_request("GetScriptSchnorrSignature", device.take_sent(), &[
        GET_SCRIPT_SCHNORR_SIGNATURE_REQUEST,
    ]);

    // --- GetOneSidedMetadataSignature
    device.expect(vec![unhex(COM_AND_PUB_SIG_REPLY)]);
    let signature = ledger_get_one_sided_metadata_signature(
        ACCOUNT,
        NETWORK,
        TXO_VERSION,
        VALUE,
        SENDER_OFFSET_KEY_INDEX,
        &scalar(0xb8),
        &receiver_address(),
        &[0xb9; 32],
    )
    .unwrap();
    expected_com_and_pub(&signature);
    assert_request("GetOneSidedMetadataSignature", device.take_sent(), &[
        GET_ONE_SIDED_METADATA_SIGNATURE_REQUEST,
    ]);

    // --- GenerateEphemeralNonce
    device.expect(vec![unhex(EPHEMERAL_NONCE_REPLY)]);
    let (handle, public_nonce) = ledger_generate_ephemeral_nonce(ACCOUNT).unwrap();
    assert_eq!(handle, NONCE_HANDLE);
    assert_eq!(public_nonce, point(0xa5));
    assert_request("GenerateEphemeralNonce", device.take_sent(), &[
        GENERATE_EPHEMERAL_NONCE_REQUEST,
    ]);

    // --- GetRawSchnorrSignature
    device.expect(vec![unhex(SCHNORR_REPLY)]);
    let signature =
        ledger_get_raw_schnorr_signature(ACCOUNT, INDEX, LedgerKeyBranch::Random, NONCE_HANDLE, &[0xba; 64]).unwrap();
    assert_eq!(signature.get_compressed_public_nonce(), &point(0xa3));
    assert_eq!(signature.get_signature(), &scalar(0xa4));
    assert_request("GetRawSchnorrSignature", device.take_sent(), &[
        GET_RAW_SCHNORR_SIGNATURE_REQUEST,
    ]);

    // --- GetRawSchnorrSignatureLegacyNonce
    device.expect(vec![unhex(SCHNORR_REPLY)]);
    let signature = ledger_get_raw_schnorr_signature_legacy_nonce(
        ACCOUNT,
        INDEX,
        LedgerKeyBranch::PreMine,
        NONCE_INDEX,
        LedgerKeyBranch::Random,
        &[0xbb; 64],
    )
    .unwrap();
    assert_eq!(signature.get_compressed_public_nonce(), &point(0xa3));
    assert_eq!(signature.get_signature(), &scalar(0xa4));
    assert_request("GetRawSchnorrSignatureLegacyNonce", device.take_sent(), &[
        GET_RAW_SCHNORR_SIGNATURE_LEGACY_NONCE_REQUEST,
    ]);

    // --- GetScriptOffset: every chunk but the last is answered with an empty `Ok`, as the device does.
    let mut replies = vec![Vec::new(); GET_SCRIPT_OFFSET_REQUEST.len().saturating_sub(1)];
    replies.push(unhex(SCRIPT_OFFSET_REPLY));
    device.expect(replies);
    let (script_offset, sender_offset_indexes) = ledger_get_script_offset(
        ACCOUNT,
        &scalar(0xbc),
        &[scalar(0xbd), scalar(0xbe)],
        &[
            (LedgerKeyBranch::PreMine, INDEX),
            (LedgerKeyBranch::PreMine, NONCE_INDEX),
        ],
        3,
    )
    .unwrap();
    assert_eq!(script_offset, scalar(0xa6));
    assert_eq!(sender_offset_indexes, vec![
        BASE_INDEX,
        BASE_INDEX.wrapping_add(1),
        BASE_INDEX.wrapping_add(2)
    ]);
    assert_request("GetScriptOffset", device.take_sent(), &GET_SCRIPT_OFFSET_REQUEST);
}

/// The frozen replies are built from the same fixed inputs the table checks the parsed values against. Pinned here
/// separately so that a reply vector cannot drift from the inputs it claims to encode.
#[test]
fn the_golden_replies_are_the_fixed_inputs_in_layout_order() {
    let version = [RESPONSE_VERSION];
    let cat = |parts: &[&[u8]]| parts.concat().to_hex();

    assert_eq!(KEY_REPLY, cat(&[&version, point(0xa1).as_bytes()]));
    assert_eq!(VIEW_KEY_REPLY, cat(&[&version, scalar(0xa2).as_bytes()]));
    assert_eq!(
        SCHNORR_REPLY,
        cat(&[&version, point(0xa3).as_bytes(), scalar(0xa4).as_bytes()])
    );
    assert_eq!(
        COM_AND_PUB_SIG_REPLY,
        cat(&[
            &version,
            point(0xc1).as_bytes(),
            point(0xc2).as_bytes(),
            scalar(0xc3).as_bytes(),
            scalar(0xc4).as_bytes(),
            scalar(0xc5).as_bytes(),
        ])
    );
    assert_eq!(
        EPHEMERAL_NONCE_REPLY,
        cat(&[&version, &NONCE_HANDLE.to_le_bytes(), point(0xa5).as_bytes()])
    );
    assert_eq!(
        SCRIPT_OFFSET_REPLY,
        cat(&[&version, scalar(0xa6).as_bytes(), &BASE_INDEX.to_le_bytes()])
    );
}

/// The payload of a golden request: everything after `cla | ins | p1 | p2 | lc`, which is what the device's handler
/// is handed.
fn payload(golden: &str) -> Vec<u8> {
    unhex(golden).split_off(5)
}

fn key_array(key: &impl ByteArray) -> [u8; 32] {
    key.as_bytes().try_into().expect("32 byte key")
}

/// The other half of the contract: the shared codec - which the device application decodes requests and encodes
/// replies with - reads exactly the golden request bytes back into the inputs the host encoded, and writes exactly
/// the golden reply bytes from the values the host parsed out of them.
///
/// The request table above proves the host still sends these bytes; this proves the device still reads them the same
/// way. Between the two, a layout cannot move on one side only.
#[test]
#[allow(clippy::too_many_lines)]
fn the_codec_reads_every_golden_request_and_writes_every_golden_reply() {
    // --- GetVersion / GetAppName: never decoded by the device, so check the encoder against the payload instead,
    //     with the same zeroed account the request table compares against.
    assert_eq!(GetVersionRequest { account: 0 }.to_vec(), payload(GET_VERSION_REQUEST));
    assert_eq!(GetAppNameRequest { account: 0 }.to_vec(), payload(GET_APP_NAME_REQUEST));

    // --- GetPublicSpendKey
    assert_eq!(
        GetPublicSpendKeyRequest::decode(&payload(GET_PUBLIC_SPEND_KEY_REQUEST)),
        Ok(GetPublicSpendKeyRequest { account: ACCOUNT })
    );

    // --- GetDHSharedSecret
    let dh_point = key_array(&point(0xb1));
    assert_eq!(
        GetDHSharedSecretRequest::decode(&payload(GET_DH_SHARED_SECRET_REQUEST)),
        Ok(GetDHSharedSecretRequest {
            account: ACCOUNT,
            index: INDEX,
            branch: u64::from(LedgerKeyBranch::OneSidedSenderOffset.as_byte()),
            public_key: &dh_point,
        })
    );

    // --- GetPublicKey
    assert_eq!(
        GetPublicKeyRequest::decode(&payload(GET_PUBLIC_KEY_REQUEST)),
        Ok(GetPublicKeyRequest {
            account: ACCOUNT,
            index: INDEX,
            branch: u64::from(LedgerKeyBranch::Random.as_byte()),
        })
    );

    // --- GetViewKey
    assert_eq!(
        GetViewKeyRequest::decode(&payload(GET_VIEW_KEY_REQUEST)),
        Ok(GetViewKeyRequest { account: ACCOUNT })
    );

    // --- GetScriptSignatureManaged / GetScriptSignatureDerived
    let (value, commitment_private_key, commitment_bytes, message) = (
        key_array(&scalar(0xb2)),
        key_array(&scalar(0xb3)),
        key_array(&commitment(0xb4)),
        [0xb5; 32],
    );
    let common = ScriptSignatureCommon {
        account: ACCOUNT,
        network: u64::from(NETWORK.as_byte()),
        txi_version: u64::from(TXO_VERSION),
        value: &value,
        commitment_private_key: &commitment_private_key,
        commitment: &commitment_bytes,
        message: &message,
    };
    let managed = payload(GET_SCRIPT_SIGNATURE_MANAGED_REQUEST);
    assert_eq!(
        GetScriptSignatureManagedRequest::decode(&managed),
        Ok(GetScriptSignatureManagedRequest {
            common,
            branch: u64::from(LedgerKeyBranch::PreMine.as_byte()),
            index: INDEX,
        })
    );
    let blinding_factor = key_array(&scalar(0xb6));
    let derived = payload(GET_SCRIPT_SIGNATURE_DERIVED_REQUEST);
    assert_eq!(
        GetScriptSignatureDerivedRequest::decode(&derived),
        Ok(GetScriptSignatureDerivedRequest {
            common,
            blinding_factor: &blinding_factor,
        })
    );

    // --- GetScriptSchnorrSignature
    let schnorr = payload(GET_SCRIPT_SCHNORR_SIGNATURE_REQUEST);
    assert_eq!(
        GetScriptSchnorrSignatureRequest::decode(&schnorr),
        Ok(GetScriptSchnorrSignatureRequest {
            account: ACCOUNT,
            index: INDEX,
            branch: u64::from(LedgerKeyBranch::OneSidedSenderOffset.as_byte()),
            message: &[0xb7; 32],
        })
    );

    // --- GetOneSidedMetadataSignature
    let mask = key_array(&scalar(0xb8));
    let address = receiver_address().to_vec();
    let metadata = payload(GET_ONE_SIDED_METADATA_SIGNATURE_REQUEST);
    assert_eq!(
        GetOneSidedMetadataSignatureRequest::decode(&metadata),
        Ok(GetOneSidedMetadataSignatureRequest::new(
            ACCOUNT,
            u64::from(NETWORK.as_byte()),
            u64::from(TXO_VERSION),
            SENDER_OFFSET_KEY_INDEX,
            VALUE,
            &mask,
            &address,
            &[0xb9; 32],
        )
        .unwrap())
    );

    // --- GenerateEphemeralNonce
    assert_eq!(
        GenerateEphemeralNonceRequest::decode(&payload(GENERATE_EPHEMERAL_NONCE_REQUEST)),
        Ok(GenerateEphemeralNonceRequest { account: ACCOUNT })
    );

    // --- GetRawSchnorrSignature
    assert_eq!(
        GetRawSchnorrSignatureRequest::decode(&payload(GET_RAW_SCHNORR_SIGNATURE_REQUEST)),
        Ok(GetRawSchnorrSignatureRequest {
            account: ACCOUNT,
            index: INDEX,
            branch: u64::from(LedgerKeyBranch::Random.as_byte()),
            nonce_handle: NONCE_HANDLE,
            challenge: &[0xba; 64],
        })
    );

    // --- GetRawSchnorrSignatureLegacyNonce
    assert_eq!(
        GetRawSchnorrSignatureLegacyNonceRequest::decode(&payload(GET_RAW_SCHNORR_SIGNATURE_LEGACY_NONCE_REQUEST)),
        Ok(GetRawSchnorrSignatureLegacyNonceRequest {
            account: ACCOUNT,
            key_index: INDEX,
            key_branch: u64::from(LedgerKeyBranch::PreMine.as_byte()),
            nonce_index: NONCE_INDEX,
            nonce_branch: u64::from(LedgerKeyBranch::Random.as_byte()),
            challenge: &[0xbb; 64],
        })
    );

    // --- Replies
    assert_eq!(
        TextReply {
            text: MIN_LEDGER_APP_VERSION.as_bytes()
        }
        .to_vec(),
        MIN_LEDGER_APP_VERSION.as_bytes()
    );
    assert_eq!(KeyReply::new(&key_array(&point(0xa1))).to_vec(), unhex(KEY_REPLY));
    assert_eq!(KeyReply::new(&key_array(&scalar(0xa2))).to_vec(), unhex(VIEW_KEY_REPLY));
    assert_eq!(
        SchnorrReply::new(&key_array(&point(0xa3)), &key_array(&scalar(0xa4))).to_vec(),
        unhex(SCHNORR_REPLY)
    );
    assert_eq!(
        ComAndPubSigReply::new(
            &key_array(&point(0xc1)),
            &key_array(&point(0xc2)),
            &key_array(&scalar(0xc3)),
            &key_array(&scalar(0xc4)),
            &key_array(&scalar(0xc5)),
        )
        .to_vec(),
        unhex(COM_AND_PUB_SIG_REPLY)
    );
    assert_eq!(
        EphemeralNonceReply::new(NONCE_HANDLE, &key_array(&point(0xa5))).to_vec(),
        unhex(EPHEMERAL_NONCE_REPLY)
    );
}
