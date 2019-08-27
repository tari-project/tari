//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::time::Duration;

/// The maximum number of transactions that can be stored in the Unconfirmed Transaction pool
pub const MEMPOOL_UNCONFIRMED_POOL_STORAGE_CAPACITY: usize = 1000;
/// The maximum number of transactions that can be skipped when compiling a set of highest priority transactions,
/// skipping over large transactions are performed in an attempt to fit more transactions into the remaining space.
pub const MEMPOOL_UNCONFIRMED_POOL_WEIGHT_TRANSACTION_SKIP_COUNT: usize = 20;

/// The maximum number of transactions that can be stored in the Orphan pool
pub const MEMPOOL_ORPHAN_POOL_STORAGE_CAPACITY: usize = 1000;
/// The time-to-live duration used for transactions stored in the OrphanPool
pub const MEMPOOL_ORPHAN_POOL_CACHE_TTL: Duration = Duration::from_secs(300);
