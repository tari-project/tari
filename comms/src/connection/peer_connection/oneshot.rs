// Copyright 2019, The Tari Project
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

use std::{
    mem,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

/// Create a synchronous oneshot channel pair
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let mutex = Arc::new(Mutex::new(State::None));
    let cond = Arc::new(Condvar::new());
    (
        Sender {
            mutex: mutex.clone(),
            cond: cond.clone(),
        },
        Receiver { mutex, cond },
    )
}

#[derive(Debug)]
pub struct Sender<T> {
    mutex: Arc<Mutex<State<T>>>,
    cond: Arc<Condvar>,
}

impl<T> Sender<T> {
    /// Send an item on the oneshhot, consuming this sender. If the receiver has
    /// dropped, the sent item is returned.
    pub fn send(self, item: T) -> Result<(), T> {
        let mut lock = acquire_lock!(self.mutex);
        if lock.is_dead() {
            return Err(item);
        }

        *lock = State::Some(item);
        self.cond.notify_one();
        Ok(())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut lock = acquire_lock!(self.mutex);
        if !lock.is_some() {
            *lock = State::Dead;
        }
        self.cond.notify_one();
    }
}

#[derive(Debug)]
enum State<T> {
    Dead,
    Used,
    None,
    Some(T),
}

impl<T> State<T> {
    fn is_some(&self) -> bool {
        match self {
            State::Some(_) => true,
            _ => false,
        }
    }

    fn is_dead(&self) -> bool {
        match self {
            State::Dead => true,
            _ => false,
        }
    }

    #[cfg(test)]
    fn take(self) -> Option<T> {
        match self {
            State::Some(v) => Some(v),
            _ => None,
        }
    }
}

pub struct Receiver<T> {
    mutex: Arc<Mutex<State<T>>>,
    cond: Arc<Condvar>,
}

impl<T> Receiver<T> {
    #[cfg(test)]
    pub fn recv(&self) -> Result<T, ()> {
        let mut guard = self.mutex.lock().map_err(|_| ())?;
        if guard.is_some() {
            let t = mem::replace(&mut *guard, State::Used);
            return Ok(t.take().expect("already checked"));
        }

        let mut guard = self.cond.wait(guard).map_err(|_| ())?;
        let state = mem::replace(&mut *guard, State::None);
        Self::get_result(state)?.ok_or(())
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<T>, ()> {
        let mut guard = self.mutex.lock().map_err(|_| ())?;

        if guard.is_dead() {
            return Err(());
        }
        if guard.is_some() {
            let state = mem::replace(&mut *guard, State::Used);
            return Self::get_result(state);
        }

        let (mut guard, timeout) = self.cond.wait_timeout(guard, timeout).map_err(|_| ())?;
        if timeout.timed_out() {
            return Ok(None);
        }
        let state = mem::replace(&mut *guard, State::Used);
        Self::get_result(state)
    }

    fn get_result(state: State<T>) -> Result<Option<T>, ()> {
        match state {
            State::Some(v) => Ok(Some(v)),
            State::None => Ok(None),
            State::Dead => Err(()),
            State::Used => unreachable!(),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        *acquire_lock!(self.mutex) = State::Dead;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;

    #[test]
    fn simple() {
        let (tx, rx) = channel();
        tx.send(123).unwrap();
        assert_eq!(rx.recv().unwrap(), 123);
    }

    #[test]
    fn simple_after() {
        let (tx, rx) = channel();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1));
            // Happens after recv is called
            tx.send(123).unwrap();
        });

        assert_eq!(rx.recv().unwrap(), 123);
    }

    #[test]
    fn timeout() {
        let (tx, rx) = channel();
        tx.send(123).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), Some(123));
    }

    #[test]
    fn timeout_after() {
        let (tx, rx) = channel();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1));
            // Happens after recv is called
            tx.send(123).unwrap();
        });

        assert_eq!(rx.recv_timeout(Duration::from_millis(100)).unwrap(), Some(123));
    }

    #[test]
    fn sender_dropped() {
        let (tx, rx) = channel::<()>();

        drop(tx);

        assert!(rx.recv_timeout(Duration::from_millis(10000)).is_err());
    }

    #[test]
    fn sender_dropped_after() {
        let (tx, rx) = channel::<()>();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(1));
            // Happens after recv is called
            drop(tx);
        });

        assert!(rx.recv_timeout(Duration::from_millis(10000)).is_err());
    }

    #[test]
    fn receiver_dropped() {
        let (tx, rx) = channel();
        drop(rx);

        assert!(tx.send(123).is_err());
    }
}
