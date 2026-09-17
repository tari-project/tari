// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The one place in this crate that builds an APDU, and the one place Spec 0 has to change.
//!
//! # Why a single helper rather than `Command::build_command` at every call site
//!
//! Most of the scenario suite cannot go through `minotari_ledger_wallet_comms::accessor_methods`. The accessors are
//! the *mirror* of the device's rules - `ledger_get_script_offset` refuses a zero sender offset count before it
//! opens the transport, `ledger_get_raw_schnorr_signature_legacy_nonce` refuses a bad branch pair before it opens
//! the transport - and a probe that the host refused never reaches the device at all. Since the device's check is
//! the one that counts (see [`minotari_ledger_wallet_common::legacy_nonce`], whose docs say so in as many words),
//! every rejection scenario has to be able to put bytes on the wire that no accessor would send.
//!
//! That is what this module is for, and it is deliberately the *only* thing in this crate that does it.
//!
//! # Rejections here, happy paths through the accessors
//!
//! The division is strict and runs the other way for anything the device accepts. A **happy path** scenario drives
//! the accessor method the console wallet actually calls, because those accessors are shipped code with real logic
//! in them - `ledger_get_script_offset` decides the chunk layout and walks `sender_offset_index`,
//! `ledger_get_script_signature` picks its instruction off a `ScriptSignatureKey` variant - and a suite that
//! re-implemented that for its own happy path would stay green while the shipped version regressed.
//!
//! So: if the device is meant to refuse it, build it here. If the device is meant to accept it, send it the way
//! the wallet does.
//!
//! # The Spec 0 seam
//!
//! A shared wire-format codec is a separate piece of work. When it lands, this module is reimplemented over it and
//! **the scenarios do not change** - they name instructions and fields, never byte offsets, because every byte
//! offset in the suite is behind one of the `payload` builders below.
//!
//! The dependency runs the opposite way to how it first looks. This module plus the scenario suite is what makes
//! capturing golden vectors for that codec cheap: a request the device accepts is already built here, and a reply
//! the device sent is already parsed here.
//!
//! # These requests do not verify the application
//!
//! Every accessor method starts with `verify_ledger_application()`. [`send`] does not, on purpose: verification is
//! itself a sequence of instructions, so a malformed-APDU probe that triggered it would interleave five unrelated
//! exchanges into whatever device state the scenario was setting up - and the script offset context is reset by
//! exactly that. The `handshake` scenarios verify the application once, first, and everything after them relies on
//! that having happened.

use ledger_transport::APDUCommand;
use minotari_ledger_wallet_common::common_types::{AppSW, Instruction, LedgerKeyBranch};
use minotari_ledger_wallet_comms::{
    error::LedgerDeviceError,
    ledger_wallet::{Command, LedgerTransport},
};

/// The class byte the device application accepts, from `Comm::new().set_expected_cla(CLA)` in `wallet/src/main.rs`.
pub const WALLET_CLA: u8 = 0x80;

/// `ledger_device_sdk::io::StatusWords::BadCla`, which the SDK answers a wrong class byte with before the
/// application sees the command at all.
///
/// Not in [`AppSW`]: the application never chooses it, so the host's enum has no name for it and
/// `AppSW::try_from` fails on it. Compared as a raw `u16` for that reason.
pub const SW_BAD_CLA: u16 = 0x6e00;

/// The largest `p1` the device will accept as a `GetScriptOffset` chunk number, from `MAX_PAYLOADS` in
/// `wallet/src/main.rs`.
pub const MAX_PAYLOADS: u8 = 250;

/// One APDU, in the shape the device parses it.
///
/// Built by [`command`] or [`chunk`] and then, for a malformed-APDU probe, bent out of shape by one of the `with_`
/// methods. Those exist so that a probe changes exactly one field of an otherwise valid request: a hand-written
/// "bad" APDU can fail for two reasons at once and then proves nothing about either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRequest {
    pub cla: u8,
    pub ins: u8,
    pub p1: u8,
    pub p2: u8,
    pub data: Vec<u8>,
}

impl RawRequest {
    /// Send this request to whichever device the ledger client is pointed at.
    pub fn send(&self) -> Result<RawReply, LedgerDeviceError> {
        send(self)
    }

    /// Send this request over a caller supplied transport, without touching the process wide registration.
    pub fn send_with(&self, transport: &dyn LedgerTransport) -> Result<RawReply, LedgerDeviceError> {
        let answer = Command::new(self.to_apdu()).execute_with_transport(transport)?;
        Ok(RawReply::from_answer(&answer))
    }

    /// Change the class byte, for the one probe that is about the class byte.
    #[must_use]
    pub fn with_cla(mut self, cla: u8) -> Self {
        self.cla = cla;
        self
    }

    /// Change the instruction byte, so that an instruction the shared `Instruction` enum cannot name can be sent.
    #[must_use]
    pub fn with_ins(mut self, ins: u8) -> Self {
        self.ins = ins;
        self
    }

    #[must_use]
    pub fn with_p1(mut self, p1: u8) -> Self {
        self.p1 = p1;
        self
    }

    #[must_use]
    pub fn with_p2(mut self, p2: u8) -> Self {
        self.p2 = p2;
        self
    }

    /// Grow or shrink the payload to `length` bytes, keeping the prefix that is already there.
    ///
    /// Used for the length ±1 probes. Truncating keeps a well formed prefix and appending zeros keeps a well formed
    /// prefix too, so in both directions the only thing wrong with the request is its length - which is the whole
    /// point of the probe.
    #[must_use]
    pub fn with_data_length(mut self, length: usize) -> Self {
        self.data.resize(length, 0);
        self
    }

    fn to_apdu(&self) -> APDUCommand<Vec<u8>> {
        APDUCommand {
            cla: self.cla,
            ins: self.ins,
            p1: self.p1,
            p2: self.p2,
            data: self.data.clone(),
        }
    }
}

/// What the device answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawReply {
    /// The status word, as the device sent it. Kept as a `u16` rather than an [`AppSW`] because the two status
    /// words the SDK owns - [`SW_BAD_CLA`] in particular - have no [`AppSW`] name at all.
    pub status: u16,
    /// The response data, without the status word.
    pub data: Vec<u8>,
}

impl RawReply {
    fn from_answer(answer: &ledger_transport::APDUAnswer<Vec<u8>>) -> Self {
        Self {
            status: answer.retcode(),
            data: answer.data().to_vec(),
        }
    }

    /// Whether the device accepted the command.
    pub fn is_ok(&self) -> bool {
        self.status == AppSW::Ok as u16
    }

    /// The status word as an [`AppSW`], if it is one the application chose.
    pub fn app_sw(&self) -> Option<AppSW> {
        AppSW::try_from(self.status).ok()
    }

    /// The status word rendered for a failure message: the name if there is one, the hex either way.
    pub fn describe_status(&self) -> String {
        match self.app_sw() {
            Some(sw) => format!("{sw:?} ({:#06x})", self.status),
            None => format!("an unnamed status word ({:#06x})", self.status),
        }
    }
}

/// Send one APDU. See the module docs for why this does not verify the application first.
pub fn send(request: &RawRequest) -> Result<RawReply, LedgerDeviceError> {
    let answer = Command::new(request.to_apdu()).execute()?;
    Ok(RawReply::from_answer(&answer))
}

/// A single-exchange instruction, with `payload` following the account.
///
/// Goes through `Command::build_command` rather than assembling the header here, so that the account encoding this
/// suite sends is by construction the one the shipped client sends.
pub fn command(account: u64, instruction: Instruction, payload: Vec<u8>) -> RawRequest {
    from_command(&Command::<Vec<u8>>::build_command(account, instruction, payload))
}

/// One chunk of a chunked instruction, with an explicit chunk number and continuation flag.
///
/// `Command::build_chunk_command` exists precisely so that a malformed sequence - a chunk number out of order, a
/// resume after a rejection - can be sent, which `chunk_command` cannot express.
pub fn chunk(account: u64, instruction: Instruction, chunk_number: u8, more: bool, payload: Vec<u8>) -> RawRequest {
    from_command(&Command::<Vec<u8>>::build_chunk_command(
        account,
        instruction,
        chunk_number,
        more,
        payload,
    ))
}

fn from_command(command: &Command<Vec<u8>>) -> RawRequest {
    let apdu = command.to_apdu_command();
    RawRequest {
        cla: apdu.cla,
        ins: apdu.ins,
        p1: apdu.p1,
        p2: apdu.p2,
        data: apdu.data,
    }
}

/// The payload bodies, one per instruction, as the device's handlers parse them.
///
/// Every byte offset the scenario suite depends on is in here and nowhere else. A scenario says
/// `payload::public_key(index, branch)`; it never says "bytes 8..16 are the index". That is what makes the Spec 0
/// codec a drop-in replacement for this module rather than a rewrite of the suite.
///
/// The account is **not** included: `Command::build_command` prepends it, and the handlers count it as the first
/// eight bytes of `data`. The sizes named in each doc comment are the handler's own length check, which includes
/// those eight bytes.
pub mod payload {
    use super::{LedgerKeyBranch, branch_bytes};

    /// `GetPublicKey`, 24 bytes with the account: `index(8) | branch(8)`.
    pub fn public_key(index: u64, branch: LedgerKeyBranch) -> Vec<u8> {
        let mut data = index.to_le_bytes().to_vec();
        data.extend_from_slice(&branch_bytes(branch));
        data
    }

    /// `GetPublicSpendKey` and `GetViewKey`, 8 bytes with the account: nothing but the account.
    pub fn account_only() -> Vec<u8> {
        Vec::new()
    }

    /// `GenerateEphemeralNonce`, 8 bytes with the account.
    ///
    /// Spelled out separately from [`account_only`] even though the bytes are identical, because they are the same
    /// by coincidence rather than by rule: the nonce is a random scalar the host cannot influence, and if it ever
    /// took an argument this is the function that would grow one.
    pub fn generate_ephemeral_nonce() -> Vec<u8> {
        Vec::new()
    }

    /// `GetDHSharedSecret`, 56 bytes with the account: `index(8) | branch(8) | public_key(32)`.
    pub fn dh_shared_secret(index: u64, branch: LedgerKeyBranch, public_key: &[u8; 32]) -> Vec<u8> {
        let mut data = index.to_le_bytes().to_vec();
        data.extend_from_slice(&branch_bytes(branch));
        data.extend_from_slice(public_key);
        data
    }

    /// `GetScriptSchnorrSignature`, 56 bytes with the account: `index(8) | branch(8) | message(32)`.
    pub fn script_schnorr_signature(index: u64, branch: LedgerKeyBranch, message: &[u8; 32]) -> Vec<u8> {
        let mut data = index.to_le_bytes().to_vec();
        data.extend_from_slice(&branch_bytes(branch));
        data.extend_from_slice(message);
        data
    }

    /// `GetRawSchnorrSignature`, 96 bytes with the account:
    /// `index(8) | branch(8) | nonce_handle(8) | challenge(64)`.
    pub fn raw_schnorr_signature(
        index: u64,
        branch: LedgerKeyBranch,
        nonce_handle: u64,
        challenge: &[u8; 64],
    ) -> Vec<u8> {
        let mut data = index.to_le_bytes().to_vec();
        data.extend_from_slice(&branch_bytes(branch));
        data.extend_from_slice(&nonce_handle.to_le_bytes());
        data.extend_from_slice(challenge);
        data
    }

    /// `GetRawSchnorrSignatureLegacyNonce`, 104 bytes with the account:
    /// `key_index(8) | key_branch(8) | nonce_index(8) | nonce_branch(8) | challenge(64)`.
    ///
    /// The branches are taken as raw bytes rather than as [`LedgerKeyBranch`] so that a scenario can send a branch
    /// identifier the shared enum does not name, which is one of the things the device has to refuse.
    pub fn raw_schnorr_signature_legacy_nonce(
        key_index: u64,
        key_branch: u8,
        nonce_index: u64,
        nonce_branch: u8,
        challenge: &[u8; 64],
    ) -> Vec<u8> {
        let mut data = key_index.to_le_bytes().to_vec();
        data.extend_from_slice(&u64::from(key_branch).to_le_bytes());
        data.extend_from_slice(&nonce_index.to_le_bytes());
        data.extend_from_slice(&u64::from(nonce_branch).to_le_bytes());
        data.extend_from_slice(challenge);
        data
    }

    /// `GetScriptSignatureManaged`, 168 bytes with the account: the common prefix then `branch(8) | index(8)`.
    pub fn script_signature_managed(
        network: u8,
        txi_version: u8,
        value: &[u8; 32],
        commitment_private_key: &[u8; 32],
        commitment: &[u8; 32],
        message: &[u8; 32],
        branch: LedgerKeyBranch,
        index: u64,
    ) -> Vec<u8> {
        let mut data =
            script_signature_common(network, txi_version, value, commitment_private_key, commitment, message);
        data.extend_from_slice(&branch_bytes(branch));
        data.extend_from_slice(&index.to_le_bytes());
        data
    }

    /// `GetScriptSignatureDerived`, 184 bytes with the account: the common prefix then `blinding_factor(32)`.
    pub fn script_signature_derived(
        network: u8,
        txi_version: u8,
        value: &[u8; 32],
        commitment_private_key: &[u8; 32],
        commitment: &[u8; 32],
        message: &[u8; 32],
        blinding_factor: &[u8; 32],
    ) -> Vec<u8> {
        let mut data =
            script_signature_common(network, txi_version, value, commitment_private_key, commitment, message);
        data.extend_from_slice(blinding_factor);
        data
    }

    /// `network(8) | txi_version(8) | value(32) | commitment_private_key(32) | commitment(32) | message(32)`,
    /// shared by both script signature instructions - `extract_common_values` in the device's handler.
    fn script_signature_common(
        network: u8,
        txi_version: u8,
        value: &[u8; 32],
        commitment_private_key: &[u8; 32],
        commitment: &[u8; 32],
        message: &[u8; 32],
    ) -> Vec<u8> {
        let mut data = u64::from(network).to_le_bytes().to_vec();
        data.extend_from_slice(&u64::from(txi_version).to_le_bytes());
        data.extend_from_slice(value);
        data.extend_from_slice(commitment_private_key);
        data.extend_from_slice(commitment);
        data.extend_from_slice(message);
        data
    }

    /// `GetScriptOffset` chunk 0, 32 bytes with the account:
    /// `sender_offset_count(8) | script_index_count(8) | derived_script_key_count(8)`.
    pub fn script_offset_header(
        sender_offset_count: u64,
        script_index_count: u64,
        derived_script_key_count: u64,
    ) -> Vec<u8> {
        let mut data = sender_offset_count.to_le_bytes().to_vec();
        data.extend_from_slice(&script_index_count.to_le_bytes());
        data.extend_from_slice(&derived_script_key_count.to_le_bytes());
        data
    }

    /// `GetScriptOffset` chunk 1: the sum of the script private keys the host already knows.
    pub fn script_offset_partial_sum(partial_sum: &[u8; 32]) -> Vec<u8> {
        partial_sum.to_vec()
    }

    /// A `GetScriptOffset` indexed script key chunk: `branch(8) | index(8)`.
    pub fn script_offset_script_index(branch: LedgerKeyBranch, index: u64) -> Vec<u8> {
        let mut data = branch_bytes(branch).to_vec();
        data.extend_from_slice(&index.to_le_bytes());
        data
    }

    /// A `GetScriptOffset` derived script key chunk: the blinding factor the device folds into `alpha`.
    pub fn script_offset_derived_script_key(blinding_factor: &[u8; 32]) -> Vec<u8> {
        blinding_factor.to_vec()
    }
}

/// A branch identifier as the device reads it: the byte widened to a little endian `u64`.
fn branch_bytes(branch: LedgerKeyBranch) -> [u8; 8] {
    u64::from(branch.as_byte()).to_le_bytes()
}

/// The `GetScriptOffset` reply: `version(1) | script_offset(32) | base_index(8)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptOffsetReply {
    pub script_offset: [u8; 32],
    pub base_index: u64,
}

impl ScriptOffsetReply {
    /// Parse a reply, or say what was wrong with it.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        use minotari_ledger_wallet_common::script_offset::SCRIPT_OFFSET_REPLY_SIZE;

        if data.len() < SCRIPT_OFFSET_REPLY_SIZE {
            return Err(format!(
                "GetScriptOffset reply is {} bytes, expected at least {SCRIPT_OFFSET_REPLY_SIZE}",
                data.len()
            ));
        }
        let mut script_offset = [0u8; 32];
        script_offset.copy_from_slice(data.get(1..33).unwrap_or_default());
        let mut base_index = [0u8; 8];
        base_index.copy_from_slice(data.get(33..41).unwrap_or_default());
        Ok(Self {
            script_offset,
            base_index: u64::from_le_bytes(base_index),
        })
    }
}

/// The `GenerateEphemeralNonce` reply: `version(1) | handle(8) | public_nonce(32)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralNonceReply {
    pub handle: u64,
    pub public_nonce: [u8; 32],
}

impl EphemeralNonceReply {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        use minotari_ledger_wallet_common::ephemeral_nonce::EPHEMERAL_NONCE_REPLY_SIZE;

        if data.len() < EPHEMERAL_NONCE_REPLY_SIZE {
            return Err(format!(
                "GenerateEphemeralNonce reply is {} bytes, expected at least {EPHEMERAL_NONCE_REPLY_SIZE}",
                data.len()
            ));
        }
        let mut handle = [0u8; 8];
        handle.copy_from_slice(data.get(1..9).unwrap_or_default());
        let mut public_nonce = [0u8; 32];
        public_nonce.copy_from_slice(data.get(9..41).unwrap_or_default());
        Ok(Self {
            handle: u64::from_le_bytes(handle),
            public_nonce,
        })
    }
}

/// A `version(1) | public_nonce(32) | signature(32)` reply, which all three Schnorr instructions share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchnorrReply {
    pub public_nonce: [u8; 32],
    pub signature: [u8; 32],
}

impl SchnorrReply {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 65 {
            return Err(format!("Schnorr reply is {} bytes, expected at least 65", data.len()));
        }
        let mut public_nonce = [0u8; 32];
        public_nonce.copy_from_slice(data.get(1..33).unwrap_or_default());
        let mut signature = [0u8; 32];
        signature.copy_from_slice(data.get(33..65).unwrap_or_default());
        Ok(Self {
            public_nonce,
            signature,
        })
    }
}

/// A `version(1) | 160 bytes` reply, which both script signature instructions and
/// `GetOneSidedMetadataSignature` share.
///
/// The five fields are in `CommitmentAndPublicKeySignature::to_vec` order: ephemeral commitment, ephemeral public
/// key, then the three responses. `u_a` before `u_x` before `u_y` - the same order `accessor_methods` reads them
/// in, and a different order from the argument list of `sign`, which takes `a, x, y` as *secrets* rather than
/// responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComAndPubSigReply {
    pub ephemeral_commitment: [u8; 32],
    pub ephemeral_pubkey: [u8; 32],
    pub u_a: [u8; 32],
    pub u_x: [u8; 32],
    pub u_y: [u8; 32],
}

impl ComAndPubSigReply {
    /// Parse a reply as it came off the wire, response version byte and all.
    pub fn parse(what: &str, data: &[u8]) -> Result<Self, String> {
        let body = data
            .get(1..)
            .ok_or_else(|| format!("{what} reply is empty, expected 1 + 160 bytes"))?;
        Self::parse_body(what, body)
    }

    /// Parse the 160 byte body on its own.
    ///
    /// For a signature that an accessor method already parsed and handed back as a type: `to_vec` on the compressed
    /// signature reproduces exactly these 160 bytes, so one verification path serves both.
    pub fn parse_body(what: &str, body: &[u8]) -> Result<Self, String> {
        if body.len() < 160 {
            return Err(format!("{what} is {} bytes, expected at least 160", body.len()));
        }
        let field = |start: usize| {
            let mut out = [0u8; 32];
            out.copy_from_slice(body.get(start..start.saturating_add(32)).unwrap_or_default());
            out
        };
        Ok(Self {
            ephemeral_commitment: field(0),
            ephemeral_pubkey: field(32),
            u_a: field(64),
            u_x: field(96),
            u_y: field(128),
        })
    }
}

/// A `version(1) | 32 bytes` reply, which every key returning instruction shares.
pub fn parse_key_reply(what: &str, data: &[u8]) -> Result<[u8; 32], String> {
    if data.len() < 33 {
        return Err(format!("{what} reply is {} bytes, expected at least 33", data.len()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(data.get(1..33).unwrap_or_default());
    Ok(key)
}

#[cfg(test)]
mod test {
    use super::*;

    /// The account really does ride in front of the payload, little endian, on a single-exchange command. This is
    /// the assumption every `payload` builder's byte offsets are stated against.
    #[test]
    fn a_command_carries_the_account_in_front_of_the_payload() {
        let request = command(0x0102_0304_0506_0708, Instruction::GetPublicKey, vec![0xaa, 0xbb]);
        assert_eq!(request.cla, WALLET_CLA);
        assert_eq!(request.ins, Instruction::GetPublicKey.as_byte());
        assert_eq!(request.p1, 0);
        assert_eq!(request.p2, 0);
        assert_eq!(request.data, vec![8, 7, 6, 5, 4, 3, 2, 1, 0xaa, 0xbb]);
    }

    /// The account rides on chunk 0 only, and `p2` is the "more chunks follow" flag. A scenario that sent the
    /// account again on a later chunk would be sending a differently shaped request from the shipped client.
    #[test]
    fn a_chunk_carries_the_account_on_the_first_chunk_only() {
        let first = chunk(1, Instruction::GetScriptOffset, 0, true, vec![0xaa]);
        assert_eq!(first.p1, 0);
        assert_eq!(first.p2, 1);
        assert_eq!(first.data, vec![1, 0, 0, 0, 0, 0, 0, 0, 0xaa]);

        let later = chunk(1, Instruction::GetScriptOffset, 3, false, vec![0xbb]);
        assert_eq!(later.p1, 3);
        assert_eq!(later.p2, 0);
        assert_eq!(later.data, vec![0xbb]);
    }

    /// Every payload builder must produce exactly the length its handler's length check demands, counting the
    /// eight account bytes the transport prepends.
    ///
    /// These are the numbers the `WrongApduLength` probes bracket, so getting one wrong here would make those
    /// probes pass for the wrong reason - they would be off by one from a length that was already wrong.
    #[test]
    fn every_payload_is_the_length_its_handler_checks_for() {
        let branch = LedgerKeyBranch::Random;
        let key = [0u8; 32];
        let challenge = [0u8; 64];
        let account = 8;

        let cases: Vec<(&str, usize, Vec<u8>)> = vec![
            ("GetPublicKey", 24, payload::public_key(1, branch)),
            ("GetPublicSpendKey / GetViewKey", 8, payload::account_only()),
            ("GenerateEphemeralNonce", 8, payload::generate_ephemeral_nonce()),
            ("GetDHSharedSecret", 56, payload::dh_shared_secret(1, branch, &key)),
            (
                "GetScriptSchnorrSignature",
                56,
                payload::script_schnorr_signature(1, branch, &key),
            ),
            (
                "GetRawSchnorrSignature",
                96,
                payload::raw_schnorr_signature(1, branch, 1, &challenge),
            ),
            (
                "GetRawSchnorrSignatureLegacyNonce",
                104,
                payload::raw_schnorr_signature_legacy_nonce(1, branch.as_byte(), 1, branch.as_byte(), &challenge),
            ),
            (
                "GetScriptSignatureManaged",
                168,
                payload::script_signature_managed(0, 0, &key, &key, &key, &key, branch, 1),
            ),
            (
                "GetScriptSignatureDerived",
                184,
                payload::script_signature_derived(0, 0, &key, &key, &key, &key, &key),
            ),
            ("GetScriptOffset header", 32, payload::script_offset_header(1, 0, 1)),
        ];

        for (name, expected, body) in cases {
            let sent = command(account, Instruction::GetPublicKey, body).data.len();
            assert_eq!(
                sent, expected,
                "{name} payload is {sent} bytes with the account, expected {expected}"
            );
        }
    }

    /// The chunks that follow the header are not account-prefixed, so their sizes are the bare field sizes the
    /// handler checks: 32 for a scalar, 16 for a branch/index pair.
    #[test]
    fn the_script_offset_body_chunks_are_the_sizes_the_handler_checks() {
        assert_eq!(payload::script_offset_partial_sum(&[0u8; 32]).len(), 32);
        assert_eq!(payload::script_offset_derived_script_key(&[0u8; 32]).len(), 32);
        assert_eq!(
            payload::script_offset_script_index(LedgerKeyBranch::PreMine, 1).len(),
            16
        );
    }

    /// A branch reaches the device as its byte widened into a little endian `u64`, which is what
    /// `branch_key_from_u64` reads back. A big endian widening would make every branch look like `Spend` or like
    /// nothing at all.
    #[test]
    fn a_branch_is_a_little_endian_u64() {
        assert_eq!(branch_bytes(LedgerKeyBranch::PreMine), [0x09, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(branch_bytes(LedgerKeyBranch::Spend), [0x07, 0, 0, 0, 0, 0, 0, 0]);
    }

    /// The `with_` helpers change exactly one field and leave everything else alone. A probe built on one of these
    /// is otherwise a request the device would have accepted, which is what makes its rejection attributable.
    #[test]
    fn the_probe_helpers_change_one_field_each() {
        let base = command(
            1,
            Instruction::GetPublicKey,
            payload::public_key(2, LedgerKeyBranch::Random),
        );

        let bad_cla = base.clone().with_cla(0x81);
        assert_eq!(bad_cla.cla, 0x81);
        assert_eq!(bad_cla.data, base.data);
        assert_eq!(bad_cla.ins, base.ins);

        let short = base.clone().with_data_length(base.data.len().saturating_sub(1));
        assert_eq!(short.data.len(), base.data.len() - 1);
        assert_eq!(short.data, base.data.get(..base.data.len() - 1).unwrap());

        let long = base.clone().with_data_length(base.data.len().saturating_add(1));
        assert_eq!(long.data.len(), base.data.len() + 1);
        assert!(long.data.starts_with(&base.data));
    }

    /// A reply that is too short must say so rather than panic, because a device that answers `WrongApduLength`
    /// answers with no data at all and every parser here is reached on that path.
    #[test]
    fn a_short_reply_is_an_error_rather_than_a_panic() {
        assert!(ScriptOffsetReply::parse(&[]).is_err());
        assert!(EphemeralNonceReply::parse(&[2, 0]).is_err());
        assert!(SchnorrReply::parse(&[2; 64]).is_err());
        assert!(parse_key_reply("GetViewKey", &[2; 32]).is_err());
    }

    #[test]
    fn the_replies_parse_at_the_offsets_the_handlers_write() {
        let mut nonce = vec![2u8];
        nonce.extend_from_slice(&7u64.to_le_bytes());
        nonce.extend_from_slice(&[0xab; 32]);
        let parsed = EphemeralNonceReply::parse(&nonce).unwrap();
        assert_eq!(parsed.handle, 7);
        assert_eq!(parsed.public_nonce, [0xab; 32]);

        let mut offset = vec![2u8];
        offset.extend_from_slice(&[0xcd; 32]);
        offset.extend_from_slice(&9u64.to_le_bytes());
        let parsed = ScriptOffsetReply::parse(&offset).unwrap();
        assert_eq!(parsed.script_offset, [0xcd; 32]);
        assert_eq!(parsed.base_index, 9);

        let mut schnorr = vec![2u8];
        schnorr.extend_from_slice(&[0x11; 32]);
        schnorr.extend_from_slice(&[0x22; 32]);
        let parsed = SchnorrReply::parse(&schnorr).unwrap();
        assert_eq!(parsed.public_nonce, [0x11; 32]);
        assert_eq!(parsed.signature, [0x22; 32]);

        // Five distinct fillers, so that a parser reading two fields from the same offset is visible here rather
        // than as a signature that mysteriously fails to verify.
        let mut com_and_pub = vec![2u8];
        for filler in [0x31u8, 0x32, 0x33, 0x34, 0x35] {
            com_and_pub.extend_from_slice(&[filler; 32]);
        }
        let parsed = ComAndPubSigReply::parse("GetScriptSignature", &com_and_pub).unwrap();
        assert_eq!(parsed.ephemeral_commitment, [0x31; 32]);
        assert_eq!(parsed.ephemeral_pubkey, [0x32; 32]);
        assert_eq!(parsed.u_a, [0x33; 32]);
        assert_eq!(parsed.u_x, [0x34; 32]);
        assert_eq!(parsed.u_y, [0x35; 32]);
        assert!(ComAndPubSigReply::parse("GetScriptSignature", &com_and_pub[..160]).is_err());
        // The two entry points must agree, or a signature that came back through an accessor method would be
        // verified against different bytes from one read straight off the wire.
        assert_eq!(
            ComAndPubSigReply::parse_body("GetScriptSignature", &com_and_pub[1..]).unwrap(),
            parsed
        );
    }

    /// A status word the application owns gets its name printed; one the SDK owns gets its hex. Both matter in a
    /// failure message, and `SW_BAD_CLA` is precisely the case with no name.
    #[test]
    fn a_status_word_describes_itself_with_or_without_a_name() {
        let named = RawReply {
            status: AppSW::BadBranchKey as u16,
            data: Vec::new(),
        };
        assert!(named.describe_status().contains("BadBranchKey"));
        assert_eq!(named.app_sw(), Some(AppSW::BadBranchKey));
        assert!(!named.is_ok());

        let unnamed = RawReply {
            status: SW_BAD_CLA,
            data: Vec::new(),
        };
        assert_eq!(unnamed.app_sw(), None);
        assert!(unnamed.describe_status().contains("0x6e00"));
    }
}
