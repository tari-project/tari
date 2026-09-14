// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! `verify_ledger_application` must actually verify, for every caller, even under concurrency.
//!
//! It used to guard its cache with `try_lock`: the first caller took the lock and talked to the device, and a
//! second caller arriving while that was still in flight failed the `try_lock`, fell through to the trailing
//! `Ok(())`, and proceeded to use a device nothing had checked. The key manager calls this from async,
//! multi-threaded context, so that was reachable in normal operation.
//!
//! This test reproduces exactly that: N threads release from a barrier together, the stub device holds the very
//! first exchange open long enough that they are all guaranteed to arrive mid-verification, and every thread
//! asserts that the whole verification sequence had already been sent before its own call returned `Ok(())`.
//!
//! This is one test in one file on purpose. `VERIFIED` and the registered transport are both process wide, so a
//! second test in the same binary would race with this one over them.

#![cfg(feature = "test_transport")]

use std::{
    collections::HashMap,
    sync::{
        Arc,
        Barrier,
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use ledger_transport::{APDUAnswer, APDUCommand};
use minotari_ledger_wallet_common::common_types::Instruction;
use minotari_ledger_wallet_comms::{
    accessor_methods::verify_ledger_application,
    error::LedgerDeviceError,
    ledger_wallet::{
        EXPECTED_NAME,
        EXPECTED_RESPONSE_VERSION,
        LedgerTransport,
        MIN_LEDGER_APP_VERSION,
        register_transport,
    },
};
use tari_crypto::{
    keys::{PublicKey, SecretKey},
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
};
use tari_script::CheckSigSchnorrSignature;
use tari_utilities::ByteArray;

const THREADS: usize = 8;
const STATUS_OK: u16 = 0x9000;

/// What a successful `verify()` sends, in order: the name, the version, a signature, the public key that signature
/// should verify against, and a second signature that has to differ from the first.
const EXPECTED_SEQUENCE: [Instruction; 5] = [
    Instruction::GetAppName,
    Instruction::GetVersion,
    Instruction::GetScriptSchnorrSignature,
    Instruction::GetPublicKey,
    Instruction::GetScriptSchnorrSignature,
];

/// A stand-in for a well behaved Ledger device, good enough to pass `verify()`.
///
/// It holds a key per `(account, index, branch)` so that the public key it hands back really is the one its
/// signatures verify against, and it signs with a fresh random nonce each time so that two signatures over the
/// same challenge differ - which `verify()` checks for.
struct StubDevice {
    /// Every instruction it has been asked for, in order.
    log: Mutex<Vec<Instruction>>,
    keys: Mutex<HashMap<Vec<u8>, RistrettoSecretKey>>,
    exchanges: AtomicUsize,
    /// How long to hold the first exchange open, to widen the window a racing caller could slip through.
    first_exchange_delay: Duration,
}

impl StubDevice {
    fn new(first_exchange_delay: Duration) -> Self {
        Self {
            log: Mutex::new(Vec::new()),
            keys: Mutex::new(HashMap::new()),
            exchanges: AtomicUsize::new(0),
            first_exchange_delay,
        }
    }

    fn instruction_log(&self) -> Vec<Instruction> {
        self.log.lock().unwrap().clone()
    }

    /// How many exchanges have *completed*. Only bumped once the answer is built, so a caller that sees `5` here
    /// knows the whole verification sequence really has been through the device.
    fn completed_exchanges(&self) -> usize {
        self.exchanges.load(Ordering::SeqCst)
    }

    fn key_for(&self, key_id: &[u8]) -> RistrettoSecretKey {
        self.keys
            .lock()
            .unwrap()
            .entry(key_id.to_vec())
            .or_insert_with(|| RistrettoSecretKey::random(&mut rand::rng()))
            .clone()
    }
}

fn answer(data: Vec<u8>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
    let mut payload = data;
    payload.extend_from_slice(&STATUS_OK.to_be_bytes());
    APDUAnswer::from_answer(payload).map_err(|e| LedgerDeviceError::TransportExchange(e.to_string()))
}

impl LedgerTransport for StubDevice {
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        let instruction = Instruction::from_byte(command.ins)
            .unwrap_or_else(|| panic!("Stub device got an unknown instruction {:#04x}", command.ins));

        let first = {
            let mut log = self.log.lock().unwrap();
            log.push(instruction);
            log.len() == 1
        };
        if first {
            // Hold the very first exchange open. Every other thread is now certain to be inside
            // `verify_ledger_application` while verification is still unfinished.
            thread::sleep(self.first_exchange_delay);
        }

        let data = match instruction {
            Instruction::GetAppName => EXPECTED_NAME.as_bytes().to_vec(),
            Instruction::GetVersion => MIN_LEDGER_APP_VERSION.as_bytes().to_vec(),
            Instruction::GetPublicKey => {
                // account(8) | index(8) | branch(8)
                assert_eq!(command.data.len(), 24, "GetPublicKey payload");
                let secret = self.key_for(&command.data);
                let mut data = vec![EXPECTED_RESPONSE_VERSION];
                data.extend_from_slice(RistrettoPublicKey::from_secret_key(&secret).as_bytes());
                data
            },
            Instruction::GetScriptSchnorrSignature => {
                // account(8) | index(8) | branch(8) | challenge(32)
                assert_eq!(command.data.len(), 56, "GetScriptSchnorrSignature payload");
                let secret = self.key_for(command.data.get(..24).expect("checked above"));
                let challenge = command.data.get(24..56).expect("checked above");
                let signature = CheckSigSchnorrSignature::sign_with_nonce_and_message(
                    &secret,
                    RistrettoSecretKey::random(&mut rand::rng()),
                    challenge,
                )
                .unwrap();
                let mut data = vec![EXPECTED_RESPONSE_VERSION];
                data.extend_from_slice(signature.get_public_nonce().as_bytes());
                data.extend_from_slice(signature.get_signature().as_bytes());
                data
            },
            other => panic!("Stub device was not expecting {other:?}"),
        };

        let answer = answer(data);
        self.exchanges.fetch_add(1, Ordering::SeqCst);
        answer
    }
}

#[test]
fn concurrent_verify_ledger_application_callers_all_verify() {
    let device = Arc::new(StubDevice::new(Duration::from_millis(500)));
    register_transport(device.clone());

    let barrier = Arc::new(Barrier::new(THREADS));
    let handles = (0..THREADS)
        .map(|_| {
            let barrier = barrier.clone();
            let device = device.clone();
            thread::spawn(move || {
                barrier.wait();
                let result = verify_ledger_application();
                // Sampled *after* the call returned: how much of the verification had actually happened by the
                // time this caller was told everything was fine.
                (result, device.completed_exchanges())
            })
        })
        .collect::<Vec<_>>();

    for (index, handle) in handles.into_iter().enumerate() {
        let (result, exchanges_seen) = handle.join().expect("verification thread panicked");
        assert!(result.is_ok(), "thread {index} failed verification: {result:?}");
        assert!(
            exchanges_seen >= EXPECTED_SEQUENCE.len(),
            "thread {index} got Ok(()) after only {exchanges_seen} of {} verification exchanges - it was told the \
             device was verified before it had been",
            EXPECTED_SEQUENCE.len()
        );
    }

    // Verification ran exactly once: the first caller did it, the rest waited for it and then shared its result.
    assert_eq!(
        device.instruction_log(),
        EXPECTED_SEQUENCE.to_vec(),
        "the verification sequence did not run exactly once"
    );

    // And the cached success is what later callers get, without touching the device again.
    assert!(verify_ledger_application().is_ok());
    assert_eq!(device.instruction_log(), EXPECTED_SEQUENCE.to_vec());
}
