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

use prost_types::Timestamp;
use std::convert::TryInto;
use tari_crypto::tari_utilities::epoch_time::EpochTime;

/// Tries to convert a series of `T`s to `U`s, returning an error at the first failure
pub fn try_convert_all<T, U, I>(into_iter: I) -> Result<Vec<U>, T::Error>
where
    I: IntoIterator<Item = T>,
    T: TryInto<U>,
{
    let iter = into_iter.into_iter();
    let mut result = Vec::with_capacity(iter.size_hint().0);
    for item in iter {
        result.push(item.try_into()?);
    }
    Ok(result)
}

/// Utility function that converts a `prost::Timestamp` to a `chrono::DateTime`
pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> EpochTime {
    (timestamp.seconds as u64).into()
}

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
pub(crate) fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}
