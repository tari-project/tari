// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Everything the tests share: the device, the ledger-mode key manager, and a window onto the wire.
//!
//! # The bootstrap is the console wallet's, line for line
//!
//! [`Device::key_manager_for`] is `applications/minotari_console_wallet/src/init/mod.rs` with the prompts taken out:
//! ask the device for `alpha * G` and for the view key, and hand both to `LedgerWallet::new` with the account and
//! the network. The network is the host's own consensus network, because that is what the host side challenges in
//! `transaction_components` hash with; a device told a different one would sign challenges nobody here could check.
//!
//! # Watching the wire, and cutting into it
//!
//! The key manager is the unit under test, but several of the properties it owes are about what it does *not*
//! send: "refused before the transport is opened" is only a claim until something counts the exchanges. So each
//! test can put an [`Observer`] between the ledger client and the simulator for the length of one closure: it
//! records every exchange, in wire order, and can run one extra piece of work immediately before a chosen exchange
//! goes out. That second ability is what lets the script offset tests land an unrelated instruction *between two
//! chunks of one key manager call* - deterministically, with no threads and no timing.
//!
//! The observer is registered with the same `register_transport` the simulator transport is, and the simulator
//! transport is put back when the closure returns or unwinds. It is test code in a test binary, linked through a
//! dev-dependency that only exists under `cfg(tari_ledger_speculos)`; no shipped binary can reach it.

use std::{
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use ledger_transport::{APDUAnswer, APDUCommand};
use minotari_ledger_wallet_common::common_types::Instruction;
use minotari_ledger_wallet_comms::{
    accessor_methods::{ledger_get_public_spend_key, ledger_get_view_key},
    error::LedgerDeviceError,
};
use minotari_ledger_wallet_comms_testing::{
    LedgerTransport,
    SpeculosTransport,
    approver::SpeculosApprover,
    fixtures,
    register_transport,
    scenarios::vectors::identify_seed,
    seeds::SeedId,
    simulator,
};
use tari_common::configuration::Network;
use tari_transaction_components::key_manager::{
    KeyManager,
    error::KeyManagerError,
    wallet_types::{LedgerWallet, WalletType},
};

/// Only one test may drive the device at a time - within **this process**. Under nextest each test is its own
/// process and the `ledger-speculos` profile's `test-threads = 1` is what serialises them; this covers `cargo test`.
static DEVICE: Mutex<()> = Mutex::new(());

/// A panicking test poisons this, and refusing it afterwards would bury the one failure that mattered under a
/// "mutex poisoned" in every test after it.
fn device_lock() -> MutexGuard<'static, ()> {
    DEVICE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The simulator, for the length of one test.
pub struct Device {
    approver: SpeculosApprover,
    seed: SeedId,
    transport: Arc<SpeculosTransport>,
}

/// Run `test` against the simulator.
///
/// Before it: connect, refuse to go on unless the device holds one of the two published test seeds, and assert the
/// device is at its home screen. After it: wait for the device to be back at home, so that a test which strands the
/// device on a screen fails itself rather than the next test.
///
/// # Why the seed gate comes first
///
/// [`Device::key_manager_for`] reads the view key off the device, and a failing assertion here can print it. That is
/// harmless for a published test seed and not for anything else, so this uses the `comms_testing` frontends' own
/// run-level precondition - `identify_seed` - before any key manager exists. See its documentation.
pub fn with_device<T>(test: impl FnOnce(&Device) -> T) -> T {
    let _device = device_lock();
    let transport = simulator::connect();
    let seed = identify_seed().unwrap_or_else(|e| panic!("{e}"));
    let approver = SpeculosApprover::from_env().expect("SPECULOS_API_ADDRESS / SPECULOS_MODEL");
    println!(
        "Running against a {} at '{}', holding the '{}' seed",
        approver.model(),
        approver.api().address(),
        seed.name()
    );
    approver
        .expect_home()
        .unwrap_or_else(|e| panic!("the device was not at its home screen when the test started: {e}"));

    let device = Device {
        approver,
        seed,
        transport,
    };
    let result = test(&device);

    device
        .approver
        .wait_for_home()
        .unwrap_or_else(|e| panic!("the test finished without the device back at its home screen: {e}"));
    result
}

impl Device {
    pub fn approver(&self) -> &SpeculosApprover {
        &self.approver
    }

    pub fn seed(&self) -> SeedId {
        self.seed
    }

    /// A ledger-mode key manager on a fresh random account below 2^32.
    ///
    /// Random for the same reason the scenario library's inputs are: a fixed account is a question the device has
    /// already answered, and every assertion in this suite is an equation that holds for any account. Below 2^32
    /// because the legacy nonce instruction refuses larger accounts (they derive the same keys as their low word -
    /// `minotari_ledger_wallet_common::legacy_nonce::check_legacy_account`), and a wallet's account is the small
    /// number the user entered anyway. The other instructions' tolerance of large accounts is pinned by
    /// `comms_testing`'s vectors scenarios, not here.
    pub fn key_manager(&self) -> KeyManager {
        self.key_manager_for(fixtures::random_account())
    }

    /// A ledger-mode key manager on `account`, bootstrapped the way the console wallet does it.
    pub fn key_manager_for(&self, account: u64) -> KeyManager {
        let public_alpha = ledger_get_public_spend_key(account)
            .unwrap_or_else(|e| panic!("GetPublicSpendKey for account {account}: {e}"));
        let view_key = ledger_get_view_key(account).unwrap_or_else(|e| panic!("GetViewKey for account {account}: {e}"));
        let ledger = LedgerWallet::new(account, network(), public_alpha, view_key);
        KeyManager::new(WalletType::Ledger(ledger)).expect("a ledger key manager, with the ledger feature compiled in")
    }

    /// Run `work`, recording every exchange it makes with the device.
    pub fn watch<T>(&self, work: impl FnOnce() -> T) -> (T, Wire) {
        let observer = Arc::new(Observer::new(self.transport.clone(), None));
        let result = self.observed(&observer, work);
        (result, observer.wire())
    }

    /// Run `work`, and run `interruption` immediately before the first exchange that is instruction `ins` with
    /// `p1 == chunk`.
    ///
    /// `interruption` runs on the same thread, inside the transport, before the chosen exchange is forwarded - so
    /// whatever it sends reaches the device strictly between the exchange before and the chosen one. It is free to
    /// use the key manager: its own exchanges come back through the observer, which records them in order and does
    /// not fire a second time.
    ///
    /// Returns whether it fired, which a test must assert. A trigger that never matched would otherwise leave an
    /// "interleaved" test passing without ever having interleaved anything.
    pub fn interleave<T>(
        &self,
        ins: Instruction,
        chunk: u8,
        interruption: impl FnOnce() + Send + 'static,
        work: impl FnOnce() -> T,
    ) -> (T, Wire, bool) {
        let trigger = Trigger {
            ins: ins.as_byte(),
            p1: chunk,
            interruption: Box::new(interruption),
        };
        let observer = Arc::new(Observer::new(self.transport.clone(), Some(trigger)));
        let result = self.observed(&observer, work);
        let fired = lock(&observer.trigger).is_none();
        (result, observer.wire(), fired)
    }

    /// Put `observer` in front of the simulator for the length of `work`, and put the simulator back whatever
    /// happens - including a panic, which is caught only long enough to restore the transport and then resumed.
    fn observed<T>(&self, observer: &Arc<Observer>, work: impl FnOnce() -> T) -> T {
        register_transport(observer.clone());
        let result = catch_unwind(AssertUnwindSafe(work));
        register_transport(self.transport.clone());
        result.unwrap_or_else(|panic| resume_unwind(panic))
    }
}

/// The network the host side of every challenge in `transaction_components` is hashed under.
pub fn network() -> Network {
    Network::get_current_or_user_setting_or_default()
}

/// One exchange with the device, as the observer saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exchange {
    pub ins: u8,
    pub p1: u8,
    pub p2: u8,
    /// The status word, or `None` if the transport itself failed.
    pub status: Option<u16>,
}

impl Exchange {
    pub fn is(self, instruction: Instruction) -> bool {
        self.ins == instruction.as_byte()
    }
}

/// Every exchange made during one watched closure, in wire order.
#[derive(Debug, Clone, Default)]
pub struct Wire(pub Vec<Exchange>);

impl Wire {
    /// How many exchanges were instruction `instruction`.
    pub fn count(&self, instruction: Instruction) -> usize {
        self.0.iter().filter(|exchange| exchange.is(instruction)).count()
    }

    /// The exchanges that were instruction `instruction`, in order.
    pub fn of(&self, instruction: Instruction) -> Vec<Exchange> {
        self.0
            .iter()
            .copied()
            .filter(|exchange| exchange.is(instruction))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

struct Trigger {
    ins: u8,
    p1: u8,
    interruption: Box<dyn FnOnce() + Send>,
}

/// A [`LedgerTransport`] that records what passes through it and can cut in ahead of one exchange.
struct Observer {
    inner: Arc<SpeculosTransport>,
    wire: Mutex<Vec<Exchange>>,
    trigger: Mutex<Option<Trigger>>,
}

impl Observer {
    fn new(inner: Arc<SpeculosTransport>, trigger: Option<Trigger>) -> Self {
        Self {
            inner,
            wire: Mutex::new(Vec::new()),
            trigger: Mutex::new(trigger),
        }
    }

    fn wire(&self) -> Wire {
        Wire(lock(&self.wire).clone())
    }
}

impl LedgerTransport for Observer {
    fn exchange(&self, command: &APDUCommand<Vec<u8>>) -> Result<APDUAnswer<Vec<u8>>, LedgerDeviceError> {
        // Taken out under the lock and run after it is released: the interruption's own exchanges come back in
        // through this method, and must find neither lock held nor a trigger left to fire.
        let fire = {
            let mut trigger = lock(&self.trigger);
            if trigger
                .as_ref()
                .is_some_and(|trigger| trigger.ins == command.ins && trigger.p1 == command.p1)
            {
                trigger.take()
            } else {
                None
            }
        };
        if let Some(trigger) = fire {
            (trigger.interruption)();
        }

        let answer = self.inner.exchange(command);
        lock(&self.wire).push(Exchange {
            ins: command.ins,
            p1: command.p1,
            p2: command.p2,
            status: answer.as_ref().ok().map(APDUAnswer::retcode),
        });
        answer
    }
}

/// The message inside a [`KeyManagerError::LedgerError`], failing the test if it is any other kind of error.
///
/// Most device refusals reach a key manager caller as a `LedgerError` wrapping the accessor's own message, which
/// names the refusing status word. Tests assert on that name, because "the device refused" and "the device refused
/// for the reason the containment is written in terms of" are different statements.
pub fn ledger_error(error: KeyManagerError) -> String {
    match error {
        KeyManagerError::LedgerError(message) => message,
        other => panic!("expected a LedgerError from the device path, got {other:?}"),
    }
}
