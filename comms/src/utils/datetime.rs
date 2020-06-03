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

use chrono::{DateTime, NaiveTime, Utc};
use std::time::Duration;

pub fn safe_future_datetime_from_duration(duration: Duration) -> DateTime<Utc> {
    let old_duration = chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::max_value());
    Utc::now().checked_add_signed(old_duration).unwrap_or_else(|| {
        chrono::MAX_DATE
            .and_time(NaiveTime::from_hms(0, 0, 0))
            .expect("cannot fail")
    })
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs > 60 {
        let mins = secs / 60;
        if mins > 60 {
            let hours = mins / 60;
            format!("{}h {}m {}s", hours, mins % 60, secs % 60)
        } else {
            format!("{}m {}s", mins, secs % 60)
        }
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn formats_duration() {
        let s = format_duration(Duration::from_secs(5));
        assert_eq!(s, "5s");
        let s = format_duration(Duration::from_secs(23 * 60 + 10));
        assert_eq!(s, "23m 10s");
        let s = format_duration(Duration::from_secs(9 * 60 * 60 + 35 * 60 + 45));
        assert_eq!(s, "9h 35m 45s");
    }
}
