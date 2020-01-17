// Copyright 2020, The Tari Project
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
    sync::{Condvar, LockResult, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

pub fn wait_timeout_until<'a, T, F>(
    condvar: &Condvar,
    mut guard: MutexGuard<'a, T>,
    dur: Duration,
    mut condition: F,
) -> LockResult<(MutexGuard<'a, T>, bool)>
where
    F: FnMut(&mut T) -> bool,
{
    let start = Instant::now();
    loop {
        if condition(&mut *guard) {
            return Ok((guard, false));
        }
        let timeout = match dur.checked_sub(start.elapsed()) {
            Some(timeout) => timeout,
            None => return Ok((guard, true)),
        };
        guard = condvar
            .wait_timeout(guard, timeout)
            .map(|(guard, timeout)| (guard, timeout.timed_out()))
            .map_err(|err| {
                let (guard, timeout) = err.into_inner();
                PoisonError::new((guard, timeout.timed_out()))
            })?
            .0;
    }
}
