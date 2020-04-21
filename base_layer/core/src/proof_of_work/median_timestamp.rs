// Copyright 2019. The Tari Project
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

use crate::blocks::blockheader::BlockHeader;
use log::*;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

pub const LOG_TARGET: &str = "c::pow::median_timestamp";

/// Returns the median timestamp for the provided header set.
pub fn get_median_timestamp(headers: Vec<BlockHeader>) -> Option<EpochTime> {
    if headers.is_empty() {
        return None;
    }
    let height = headers.last().expect("Header set should not be empty").height;
    debug!(target: LOG_TARGET, "Calculating median timestamp to height:{}", height);
    let mut timestamps = headers.iter().map(|h| h.timestamp).collect::<Vec<_>>();
    timestamps.sort();
    trace!(target: LOG_TARGET, "Sorted median timestamps: {:?}", timestamps);
    // Calculate median timestamp
    let mid_index = timestamps.len() / 2;
    // let median_timestamp=if timestamps.len()%2==0 {
    // (timestamps[mid_index-1]+timestamps[mid_index])/2
    // }
    // else { timestamps[mid_index] };
    let median_timestamp = timestamps[mid_index];
    debug!(
        target: LOG_TARGET,
        "Median timestamp:{} at height:{}", median_timestamp, height
    );
    Some(median_timestamp)
}
