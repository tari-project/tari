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

/// Buffer size for inbound messages from _all_ peers. This should be large enough to buffer quite a few incoming
/// messages before creating backpressure on peers speaking the messaging protocol.
pub const INBOUND_MESSAGE_BUFFER_SIZE: usize = 100;
/// Buffer size for actor requests to connection manager. A lower value is ok because the connection manager shouldn't
/// need to handle a ton of requests concurrently.
pub const CONNECTION_MANAGER_REQUEST_BUFFER_SIZE: usize = 10;
/// Connection manager events buffer size. The size should allow more than enough "time" for slow subscribers to read
/// the events while not being wasteful.
pub const CONNECTION_MANAGER_EVENTS_BUFFER_SIZE: usize = 30;
/// Buffer size notifications that a peer wants to speak /tari/messaging. This buffer is used for all peers, but a low
/// value is ok because this events happen once (or less) per connecting peer. For e.g. a value of 10 would allow 10
/// peers to concurrently request to speak /tari/messaging.
pub const MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE: usize = 10;
/// Buffer size of the messaging event channel. The size should allow more than enough "time" for slow subscribers to
/// read the events while not being wasteful. An MessageReceived event is published per message, which could happen
/// quite a lot, so a little more room to buffer here is recommended.
pub const MESSAGING_EVENTS_BUFFER_SIZE: usize = 100;
/// Buffer size for requests to the messaging protocol. All outbound messages will be sent along this channel. Some
/// buffering may be required if the node needs to send many messages out at the same time.
pub const MESSAGING_REQUEST_BUFFER_SIZE: usize = 50;
