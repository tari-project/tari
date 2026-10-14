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

use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, Utc};

pub fn safe_future_datetime_from_duration(duration: Duration) -> DateTime<Utc> {
    let old_duration = chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::MAX);
    Utc::now()
        .checked_add_signed(old_duration)
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
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
        format!("{secs}s")
    }
}

pub fn format_local_datetime(datetime: &NaiveDateTime) -> String {
    let local_datetime = DateTime::<Local>::from_naive_utc_and_offset(*datetime, Local::now().offset().to_owned());
    local_datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// The latest time stored in a database: 9999-12-31 23:59:59.
///
/// Databases such as SQLite store times as text and compare them as text. A later time has a five or more digit year
/// written with a leading `+` (e.g. `+262142-12-31 ...` for [`DateTime::<Utc>::MAX_UTC`]), which sorts before every
/// four-digit year, so a ban "forever" would read as long expired. Store-bound times are clamped to this.
pub fn max_stored_datetime() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(9999, 12, 31)
        .and_then(|date| date.and_hms_opt(23, 59, 59))
        .expect("9999-12-31 23:59:59 is a valid date and time")
}

/// `datetime`, or [`max_stored_datetime`] if it is later.
pub fn clamp_to_max_stored_datetime(datetime: NaiveDateTime) -> NaiveDateTime {
    datetime.min(max_stored_datetime())
}

/// True for the "forever" time: [`DateTime::<Utc>::MAX_UTC`], or the [`max_stored_datetime`] it is stored as.
pub fn is_max_datetime(datetime: &NaiveDateTime) -> bool {
    *datetime >= max_stored_datetime()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn far_future_times_are_clamped_for_storage() {
        let forever = safe_future_datetime_from_duration(Duration::from_secs(u64::MAX)).naive_utc();
        assert!(forever > max_stored_datetime());
        let stored = clamp_to_max_stored_datetime(forever);
        assert_eq!(stored, max_stored_datetime());
        assert!(is_max_datetime(&stored));
        assert!(is_max_datetime(&forever));
        // A four-digit year: sorts after every current time as text, too
        assert_eq!(stored.to_string(), "9999-12-31 23:59:59");

        let soon = Utc::now().naive_utc();
        assert_eq!(clamp_to_max_stored_datetime(soon), soon);
        assert!(!is_max_datetime(&soon));
    }

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
