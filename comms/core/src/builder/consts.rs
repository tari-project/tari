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

/// Buffer size for actor requests to connectivity manager.
pub const CONNECTIVITY_MANAGER_REQUEST_BUFFER_SIZE: usize = 10;
/// Buffer size for connectivity events
pub const CONNECTIVITY_MANAGER_EVENTS_BUFFER_SIZE: usize = 500;
/// Buffer size for actor requests to connection manager. Must be large enough to absorb a burst of proactive dial
/// requests (see `comms::connectivity::proactive_dialer::MAX_CONCURRENT_DIALS`) without back-pressuring senders inside
/// the connectivity refresh timeout.
pub const CONNECTION_MANAGER_REQUEST_BUFFER_SIZE: usize = 60;
/// Connection manager events buffer size. The size should allow more than enough "time" for slow subscribers to read
/// the events while not being wasteful.
pub const CONNECTION_MANAGER_EVENTS_BUFFER_SIZE: usize = 30;
