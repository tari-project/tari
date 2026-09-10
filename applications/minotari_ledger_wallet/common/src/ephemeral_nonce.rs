// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! The device's ephemeral nonce store, and the wire sizes of the `GenerateEphemeralNonce` exchange.
//!
//! A Schnorr signature leaks its private key the moment two signatures over different challenges reuse a nonce:
//! `k = (s1 - s2) / (e1 - e2)`, and then `x = (s1 - k) / e1`. The nonce must therefore be drawn by the device and
//! must never be reachable twice, because a compromised host is free to replay whatever it can name.
//!
//! This store is what makes "never twice" true. The device draws the scalar, keeps it, and hands back an opaque
//! handle; the handle is the only thing the host ever sees, and taking a nonce out of the store is the same
//! operation as reading it. There is no way to read a slot without emptying it.
//!
//! Note what the store does *not* try to guarantee: that a reserved nonce is eventually signed with. A caller
//! reserves, does other work - which on a device is more round trips, any of which the user can reject - and only
//! then signs. Every abandoned reservation is a slot nobody will ever free, and there is no release instruction to
//! free it with. So the store evicts instead of refusing; see [`EphemeralNonceStore::insert`].
//!
//! The logic lives here, rather than in the Ledger application, so that it can be tested without a device - the
//! application itself only builds for the Ledger targets. It is generic over the secret type for the same reason:
//! the device's `RistrettoSecretKey` is defined in the application crate, which this crate cannot depend on.

/// How many ephemeral nonces the device will hold at once.
///
/// The store is a fixed size array in RAM on a device with a very small stack, so this is a hard bound rather than
/// a tuning knob. Eight covers the outstanding nonces of a multi party exchange with room to spare.
///
/// Exceeding it is not an error - [`EphemeralNonceStore::insert`] evicts the oldest entry - so this is the depth
/// at which an old reservation starts going stale, not a cliff the wallet falls off.
pub const EPHEMERAL_NONCE_STORE_SIZE: usize = 8;

/// Size of the `GenerateEphemeralNonce` reply: `version(1) | handle(8) | public_nonce(32)`.
pub const EPHEMERAL_NONCE_REPLY_SIZE: usize = 41;

/// The handle value that never names a nonce.
///
/// Zero is reserved so that an empty slot is distinguishable from an occupied one without a second field, and so
/// that a host which sends a zeroed handle is refused rather than served whatever happens to sit in slot zero.
pub const INVALID_NONCE_HANDLE: u64 = 0;

/// Why an ephemeral nonce operation was refused.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EphemeralNonceStoreError {
    /// The handle counter is exhausted.
    ///
    /// This is *not* "the store is full": a full store evicts. It is the one condition that must refuse, because
    /// the alternative is re-issuing a handle that an earlier nonce still answers to.
    Full,
    /// The handle was never issued, has already been consumed, or was evicted to make room.
    InvalidHandle,
}

/// One slot of [`EphemeralNonceStore`]. `handle == INVALID_NONCE_HANDLE` means the slot is free.
struct NonceSlot<T> {
    handle: u64,
    secret: T,
}

/// A fixed size, handle keyed store of device generated secrets.
///
/// Handles are issued from a strictly increasing counter rather than derived from the slot index. If the handle
/// were the index, the slot freed by a consumed nonce would immediately answer to the same handle again as soon as
/// it was refilled, and the host could name the new nonce without ever having been told about it.
///
/// Because `INVALID_NONCE_HANDLE` is zero and issued handles start at one, "the free slots" and "the oldest slots"
/// are the same ordering. Both [`EphemeralNonceStore::insert`] cases fall out of a single minimum by handle.
pub struct EphemeralNonceStore<T: Default> {
    slots: [NonceSlot<T>; EPHEMERAL_NONCE_STORE_SIZE],
    next_handle: u64,
}

impl<T: Default> Default for EphemeralNonceStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default> EphemeralNonceStore<T> {
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| NonceSlot {
                handle: INVALID_NONCE_HANDLE,
                secret: T::default(),
            }),
            // Zero is reserved as "no handle", so the first nonce issued is handle 1.
            next_handle: 1,
        }
    }

    /// Store a secret and return the handle that names it, evicting the oldest entry if there is no free slot.
    ///
    /// # Why evict rather than refuse
    ///
    /// Nothing releases a reserved nonce except signing with it, and a caller that reserves and then fails - a
    /// transport error, a timeout, a user rejecting the next prompt - abandons the slot for the life of the store.
    /// Refusing when full turns a handful of ordinary failures into a wallet that cannot sign at all until the
    /// application is restarted, with nothing in the error to suggest that is the fix. Evicting makes the store
    /// self-healing: the next reservation reclaims the leak.
    ///
    /// It costs no safety. An evicted nonce is by definition one that was never signed with, so no signature
    /// exists over it and there is no second signature to difference against - the reuse this store prevents is
    /// unaffected. Nor does it hand the host anything: the host can already reserve as often as it likes, so it
    /// could always have pushed an old entry out. The only visible cost is that a genuinely outstanding handle can
    /// go stale and be refused by [`Self::take`] at signing time, which is the same failure the caller would have
    /// had at reservation time, moved later and made recoverable.
    ///
    /// Counter exhaustion is the one case that still refuses; see [`EphemeralNonceStoreError::Full`].
    pub fn insert(&mut self, secret: T) -> Result<u64, EphemeralNonceStoreError> {
        let handle = self.next_handle;
        // A wrapped counter would re-issue a handle that an earlier nonce still answers to, and that - unlike a
        // full store - genuinely is unsafe, so this refuses. Reaching it needs 2^64 - 1 nonces from a device that
        // is power cycled far more often.
        if handle == u64::MAX {
            return Err(EphemeralNonceStoreError::Full);
        }
        // Free slots hold `INVALID_NONCE_HANDLE`, which is below every issued handle, so the minimum is a free
        // slot whenever there is one and the oldest reservation otherwise. That is both cases at once.
        //
        // `ok_or` is unreachable in practice - `EPHEMERAL_NONCE_STORE_SIZE` is a non-zero constant, so the array
        // is never empty - and is written this way only to avoid an unwrap.
        let slot = self
            .slots
            .iter_mut()
            .min_by_key(|slot| slot.handle)
            .ok_or(EphemeralNonceStoreError::Full)?;
        slot.handle = handle;
        // Assigning over the old secret drops it, which zeroizes it for any secret type that asks to be.
        slot.secret = secret;
        self.next_handle = handle.saturating_add(1);
        Ok(handle)
    }

    /// Take the secret named by `handle`, emptying its slot.
    ///
    /// This is the only way to read a stored secret, so a nonce cannot be used twice: the second attempt finds an
    /// empty slot and is refused.
    pub fn take(&mut self, handle: u64) -> Result<T, EphemeralNonceStoreError> {
        if handle == INVALID_NONCE_HANDLE {
            return Err(EphemeralNonceStoreError::InvalidHandle);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.handle == handle)
            .ok_or(EphemeralNonceStoreError::InvalidHandle)?;
        slot.handle = INVALID_NONCE_HANDLE;
        Ok(core::mem::take(&mut slot.secret))
    }

    /// Drop every stored nonce.
    ///
    /// The handle counter is deliberately left alone: a handle that was issued before a reset must not become
    /// valid again afterwards.
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.handle = INVALID_NONCE_HANDLE;
            slot.secret = T::default();
        }
    }

    /// How many slots currently hold a nonce.
    pub fn occupied(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.handle != INVALID_NONCE_HANDLE)
            .count()
    }
}

#[cfg(test)]
mod test {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn handles_are_issued_from_one_and_never_repeat() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let first = store.insert(11).unwrap();
        let second = store.insert(22).unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(store.occupied(), 2);

        // Freeing a slot must not make its handle available to the next nonce that lands in it.
        assert_eq!(store.take(first).unwrap(), 11);
        let third = store.insert(33).unwrap();
        assert_eq!(third, 3);
        assert_eq!(store.take(third).unwrap(), 33);
    }

    #[test]
    fn a_handle_cannot_be_taken_twice() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let handle = store.insert(7).unwrap();
        assert_eq!(store.take(handle).unwrap(), 7);
        assert_eq!(store.take(handle), Err(EphemeralNonceStoreError::InvalidHandle));
        assert_eq!(store.occupied(), 0);
    }

    #[test]
    fn an_unissued_handle_is_refused() {
        let mut store = EphemeralNonceStore::<u64>::new();

        assert_eq!(store.take(1), Err(EphemeralNonceStoreError::InvalidHandle));
        assert_eq!(
            store.take(INVALID_NONCE_HANDLE),
            Err(EphemeralNonceStoreError::InvalidHandle)
        );

        let handle = store.insert(7).unwrap();
        assert_eq!(
            store.take(handle.saturating_add(1)),
            Err(EphemeralNonceStoreError::InvalidHandle)
        );
    }

    /// Nothing releases a reserved nonce except signing with it, so a caller that abandons one leaks a slot for
    /// good. Refusing when full would let a handful of ordinary failures - a user rejecting a prompt, say - wedge
    /// the store until the application restarts, so a full store evicts and keeps working.
    #[test]
    fn a_full_store_evicts_the_oldest_entry_rather_than_refusing() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let mut handles = Vec::new();
        for value in 0..EPHEMERAL_NONCE_STORE_SIZE {
            handles.push(store.insert(u64::try_from(value).unwrap()).unwrap());
        }
        assert_eq!(store.occupied(), EPHEMERAL_NONCE_STORE_SIZE);

        // The store is full, and reserving again still succeeds.
        let newest = store.insert(99).unwrap();
        assert_eq!(store.occupied(), EPHEMERAL_NONCE_STORE_SIZE);

        // The oldest handle is the one that went, and it is refused exactly like a consumed one.
        let evicted = handles.first().copied().unwrap();
        assert_eq!(store.take(evicted), Err(EphemeralNonceStoreError::InvalidHandle));

        // Every other handle survived, including the one that displaced it.
        for (offset, handle) in handles.iter().enumerate().skip(1) {
            assert_eq!(store.take(*handle).unwrap(), u64::try_from(offset).unwrap());
        }
        assert_eq!(store.take(newest).unwrap(), 99);
    }

    /// Eviction is by age, not by slot position, so it stays correct once slots have been recycled out of order.
    #[test]
    fn eviction_follows_age_after_slots_are_recycled() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let mut handles = Vec::new();
        for value in 0..EPHEMERAL_NONCE_STORE_SIZE {
            handles.push(store.insert(u64::try_from(value).unwrap()).unwrap());
        }
        // Free a slot in the middle and refill it, so the youngest nonce no longer sits in the last slot.
        let recycled = handles.get(3).copied().unwrap();
        assert_eq!(store.take(recycled).unwrap(), 3);
        let refilled = store.insert(77).unwrap();

        // Filling up again must still evict the genuinely oldest entry, not the recycled slot.
        let displacing = store.insert(88).unwrap();
        assert_eq!(
            store.take(handles.first().copied().unwrap()),
            Err(EphemeralNonceStoreError::InvalidHandle)
        );
        assert_eq!(store.take(refilled).unwrap(), 77);
        assert_eq!(store.take(displacing).unwrap(), 88);
    }

    /// The one case that must refuse rather than evict: re-issuing a handle an earlier nonce still answers to is
    /// the only thing here that would actually be unsafe.
    #[test]
    fn an_exhausted_handle_counter_refuses_instead_of_evicting() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let survivor = store.insert(5).unwrap();
        store.next_handle = u64::MAX;

        assert_eq!(store.insert(6), Err(EphemeralNonceStoreError::Full));
        // Refusing must not have disturbed what was already stored.
        assert_eq!(store.occupied(), 1);
        assert_eq!(store.take(survivor).unwrap(), 5);
    }

    #[test]
    fn reset_empties_the_store_without_reviving_handles() {
        let mut store = EphemeralNonceStore::<u64>::new();

        let handle = store.insert(5).unwrap();
        store.reset();
        assert_eq!(store.occupied(), 0);
        assert_eq!(store.take(handle), Err(EphemeralNonceStoreError::InvalidHandle));

        // The counter survived the reset, so the pre-reset handle stays dead.
        let next = store.insert(6).unwrap();
        assert!(next > handle);
    }
}
