// Copyright 2024 The Tari Project
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

#[cfg(feature = "test_transport")]
use std::sync::{Arc, RwLock};
use std::{
    ops::Deref,
    sync::{LazyLock, Mutex},
};

use ledger_transport::{APDUAnswer, APDUCommand};
use ledger_transport_hid::{TransportNativeHID, hidapi::HidApi};
use minotari_ledger_wallet_common::codec::{CLA, Request};

use crate::error::LedgerDeviceError;

pub const EXPECTED_NAME: &str = "minotari_ledger_wallet";
/// `GetRawSchnorrSignature` now takes a device issued nonce handle instead of a host chosen nonce index, so older
/// applications cannot serve this client at all. Keep this in step with the ledger application's `version` in its
/// `Cargo.toml`.
pub const MIN_LEDGER_APP_VERSION: &str = "5.7.0-pre.6";

struct HidManager {
    inner: Option<HidApi>,
}

impl HidManager {
    fn new() -> Result<Self, LedgerDeviceError> {
        let hidapi = HidApi::new().map_err(|e| LedgerDeviceError::HidApi(e.to_string()))?;
        Ok(Self { inner: Some(hidapi) })
    }

    fn refresh_if_needed(&mut self) -> Result<(), LedgerDeviceError> {
        // We need to clear out the inner HidApi instance before creating a new one
        // This is because only one instance may exist, even when it no longers holds a connection,
        // and we want this dropped before replacing
        self.inner = None;

        self.inner = Some(HidApi::new().map_err(|e| LedgerDeviceError::HidApiRefresh(e.to_string()))?);

        Ok(())
    }

    fn get_hidapi(&self) -> Option<&HidApi> {
        self.inner.as_ref()
    }
}

static HID_MANAGER: LazyLock<Mutex<HidManager>> =
    LazyLock::new(|| Mutex::new(HidManager::new().expect("Failed to initialize HidManager")));

pub fn get_transport() -> Result<TransportNativeHID, LedgerDeviceError> {
    let mut manager = HID_MANAGER
        .lock()
        .map_err(|_| LedgerDeviceError::TransportConnect("Mutex lock error".to_string()))?;

    match TransportNativeHID::new(manager.get_hidapi().unwrap()) {
        Ok(transport) => Ok(transport),
        Err(_) => {
            manager.refresh_if_needed()?;
            TransportNativeHID::new(manager.get_hidapi().unwrap())
                .map_err(|e| LedgerDeviceError::TransportConnect(e.to_string()))
        },
    }
}

/// A connection to a Ledger device that can exchange APDUs with it.
///
/// This is the seam the whole ledger client sits on: the HID transport used against real hardware and the TCP
/// transport used against a Speculos simulator are both just implementations of this trait.
///
/// # `exchange` may block indefinitely - never wrap it in a timeout
///
/// An exchange is not a request/response against a machine that is always ready to answer. Several instructions
/// put a review screen up on the device and then wait for a human: `GetOneSidedMetadataSignature` shows the
/// recipient and amount and does not reply until somebody physically presses a button or taps the screen. There is
/// no upper bound on how long that takes - the device will sit on that screen for as long as the user leaves it
/// there, and a user who walks away leaves the call outstanding indefinitely.
///
/// Therefore:
///
/// - Do not impose a timeout on `exchange`, at any layer. A timeout does not cancel the operation on the device; it
///   only abandons the reply, desynchronises the host from the device, and turns "the user was slow" into a spurious
///   error - after which the next exchange reads the previous instruction's answer.
/// - Do not call this from a context that cannot afford to block for minutes. Callers on an async runtime must move it
///   onto a blocking thread.
///
/// Implementations are also free to block on their own connection handling (reconnects, one-exchange-at-a-time
/// serialisation), which is a second reason a caller cannot assume a bounded call.
///
/// Implementations must be `Send + Sync`: a single transport is shared across threads, and it is each
/// implementation's own business to serialise concurrent exchanges over whatever connection it holds.
pub trait LedgerTransport: Send + Sync {
    /// Send one APDU command to the device and wait for its answer.
    ///
    /// May block indefinitely - see the trait documentation.
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError>;
}

impl LedgerTransport for TransportNativeHID {
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        TransportNativeHID::exchange(self, command).map_err(|e| LedgerDeviceError::TransportExchange(e.to_string()))
    }
}

/// The process wide transport override, used by the simulator test harness and by nothing else.
///
/// This is deliberately a registration function and not an environment variable or a config value. A shipped
/// `minotari_console_wallet` builds with `ledger` on by default, so anything readable from the ambient environment
/// would be a way to point a user's release binary at an attacker's "device" without touching the build. To get a
/// transport in here you have to link a crate and call a function, which means you already control the binary, and
/// at that point there is nothing left to protect.
///
/// The feature is off by default and the only crate that turns it on is
/// `minotari_ledger_wallet_comms_testing`, which is excluded from this workspace so that Cargo's per-package
/// feature unification cannot turn it on for a shipped build. See the comment on `[workspace] exclude` in the root
/// `Cargo.toml`.
#[cfg(feature = "test_transport")]
static REGISTERED_TRANSPORT: RwLock<Option<Arc<dyn LedgerTransport>>> = RwLock::new(None);

/// Point every subsequent [`Command::execute`] at `transport` instead of at a HID device.
///
/// Test only; see [`REGISTERED_TRANSPORT`]. Registering a second transport replaces the first.
#[cfg(feature = "test_transport")]
pub fn register_transport(transport: Arc<dyn LedgerTransport>) {
    let mut registered = REGISTERED_TRANSPORT.write().unwrap_or_else(|e| e.into_inner());
    *registered = Some(transport);
}

/// Remove any registered transport, so that [`Command::execute`] goes back to HID.
///
/// Test only; see [`REGISTERED_TRANSPORT`].
#[cfg(feature = "test_transport")]
pub fn clear_registered_transport() {
    let mut registered = REGISTERED_TRANSPORT.write().unwrap_or_else(|e| e.into_inner());
    *registered = None;
}

/// The currently registered test transport, if any.
#[cfg(feature = "test_transport")]
fn registered_transport() -> Option<Arc<dyn LedgerTransport>> {
    REGISTERED_TRANSPORT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(Arc::clone)
}

#[derive(Debug, Clone)]
pub struct Command<D> {
    inner: APDUCommand<D>,
}

impl Command<Vec<u8>> {
    /// The APDU for a typed request: its header and payload both come from the shared codec, so the bytes this
    /// sends are by construction the ones the device's decoder reads.
    ///
    /// For a payload the codec refuses to construct - a malformed request the device has to reject - see
    /// [`crate::raw`].
    pub fn from_request<R: Request>(request: &R) -> Self {
        Command::new(APDUCommand {
            cla: CLA,
            ins: R::INSTRUCTION.as_byte(),
            p1: request.p1(),
            p2: request.p2(),
            data: request.to_vec(),
        })
    }
}

impl<D: Deref<Target = [u8]>> Command<D> {
    pub fn new(inner: APDUCommand<D>) -> Command<D> {
        Self { inner }
    }

    /// The APDU this command sends, as an owned command.
    pub fn to_apdu_command(&self) -> APDUCommand<Vec<u8>> {
        APDUCommand {
            cla: self.inner.cla,
            ins: self.inner.ins,
            p1: self.inner.p1,
            p2: self.inner.p2,
            data: self.inner.data.to_vec(),
        }
    }

    /// Send this command to the device and wait for its answer.
    ///
    /// May block indefinitely on user interaction - see [`LedgerTransport`].
    ///
    /// A fresh HID connection is opened per exchange, which is how this has always worked and is cheap enough that
    /// reusing one is not worth the reconnect handling. Connection lifetime is each transport's own business, so a
    /// registered test transport is free to hold a persistent connection instead.
    pub fn execute(&self) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        #[cfg(feature = "test_transport")]
        if let Some(transport) = registered_transport() {
            return transport.exchange(&self.to_apdu_command());
        }

        get_transport()?
            .exchange(&self.inner)
            .map_err(|e| LedgerDeviceError::TransportExchange(e.to_string()))
    }

    /// Send this command over a caller supplied transport.
    ///
    /// May block indefinitely on user interaction - see [`LedgerTransport`].
    pub fn execute_with_transport(
        &self,
        transport: &dyn LedgerTransport,
    ) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        transport.exchange(&self.to_apdu_command())
    }
}

#[cfg(test)]
mod test {
    use minotari_ledger_wallet_common::{
        codec::{Encode, GetPublicKeyRequest, ScriptOffsetRequest},
        common_types::Instruction,
    };

    use super::*;
    use crate::raw;

    /// The bytes on the wire must not depend on which transport is carrying them.
    ///
    /// `execute` hands the HID transport `&self.inner` directly, and a registered test transport
    /// `self.to_apdu_command()`. If those two ever serialised differently, a simulator test would be exercising a
    /// different APDU from the one real hardware receives, and `test_transport` would have changed the shipped
    /// behaviour it is supposed to leave alone.
    #[test]
    fn the_owned_command_serialises_identically_to_the_borrowed_one() {
        let mut commands = vec![
            Command::from_request(&GetPublicKeyRequest {
                account: 0x0102_0304_0506_0708,
                index: 1,
                branch: 2,
            }),
            raw::build_command(0, Instruction::GetAppName, vec![0]),
            raw::build_command(u64::MAX, Instruction::GetVersion, vec![]),
            raw::build_command(0x0102_0304_0506_0708, Instruction::GetPublicKey, vec![1, 2, 3, 4]),
            raw::build_chunk_command(7, Instruction::GetScriptOffset, 0, true, vec![9, 9]),
            raw::build_chunk_command(7, Instruction::GetScriptOffset, 3, false, vec![8]),
        ];
        let blinding_factor = [5u8; 32];
        let request = ScriptOffsetRequest {
            account: 42,
            sender_offset_count: 1,
            partial_script_key_sum: &[1; 32],
            script_key_indexes: &[(9, 3)],
            derived_script_keys: &[&blinding_factor],
        };
        commands.extend(request.chunks().map(|chunk| Command::from_request(&chunk)));

        for command in &commands {
            assert_eq!(command.inner.serialize(), command.to_apdu_command().serialize());
        }
    }

    /// A typed request travels under the class byte the device accepts, the instruction its type names, and a zero
    /// `p1`/`p2`, with the codec's bytes as the payload.
    #[test]
    fn a_typed_request_carries_its_own_header() {
        let request = GetPublicKeyRequest {
            account: 1,
            index: 2,
            branch: 3,
        };
        let command = Command::from_request(&request);
        assert_eq!(command.inner.cla, CLA);
        assert_eq!(command.inner.ins, Instruction::GetPublicKey.as_byte());
        assert_eq!(command.inner.p1, 0x00);
        assert_eq!(command.inner.p2, 0x00);
        assert_eq!(command.inner.data, request.to_vec());
    }

    /// A chunk travels with its number in `p1` and "more chunks follow" in `p2`, and only the first carries the
    /// account. These are the bytes the device parses.
    #[test]
    fn a_chunk_carries_its_number_and_continuation_flag_in_the_header() {
        let blinding_factor = [0xbb; 32];
        let request = ScriptOffsetRequest {
            account: 1,
            sender_offset_count: 1,
            partial_script_key_sum: &[0xaa; 32],
            script_key_indexes: &[],
            derived_script_keys: &[&blinding_factor],
        };
        let headers = request
            .chunks()
            .map(|chunk| Command::from_request(&chunk))
            .map(|command| (command.inner.cla, command.inner.ins, command.inner.p1, command.inner.p2))
            .collect::<Vec<_>>();
        let ins = Instruction::GetScriptOffset.as_byte();
        assert_eq!(headers, vec![(CLA, ins, 0, 1), (CLA, ins, 1, 1), (CLA, ins, 2, 0)]);
    }
}
