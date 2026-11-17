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

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, macros::format_description};

pub fn safe_future_datetime_from_duration(duration: Duration) -> OffsetDateTime {
    let old_duration = time::Duration::try_from(duration).unwrap_or(time::Duration::MAX);
    OffsetDateTime::now_utc()
        .checked_add(old_duration)
        .unwrap_or_else(|| PrimitiveDateTime::MAX.assume_utc())
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

pub fn format_local_datetime(datetime: &PrimitiveDateTime) -> String {
    datetime
        .format(&format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .unwrap()
        .to_string()
}

/// The latest time stored in a database: 9999-12-31 23:59:59.
///
/// Databases such as SQLite store times as text and compare them as text. A later time has a five or more digit year
/// written with a leading `+` (e.g. `+262142-12-31 ...` for [`OffsetDateTime::MAX`]), which sorts before every
/// four-digit year, so a ban "forever" would read as long expired. Store-bound times are clamped to this.
pub fn max_stored_datetime() -> PrimitiveDateTime {
    Date::from_calendar_date(9999, Month::December, 31)
        .ok()
        .and_then(|date| date.with_hms(23, 59, 59).ok())
        .expect("9999-12-31 23:59:59 is a valid date and time")
}

/// `datetime`, or [`max_stored_datetime`] if it is later.
pub fn clamp_to_max_stored_datetime(datetime: PrimitiveDateTime) -> PrimitiveDateTime {
    datetime.min(max_stored_datetime())
}

/// True for the "forever" time: [`OffsetDateTime::MAX`], or the [`max_stored_datetime`] it is stored as.
pub fn is_max_datetime(datetime: &PrimitiveDateTime) -> bool {
    *datetime >= max_stored_datetime()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn far_future_times_are_clamped_for_storage() {
        let forever_dt = safe_future_datetime_from_duration(Duration::from_secs(u64::MAX));
        let forever = PrimitiveDateTime::new(forever_dt.date(), forever_dt.time());
        assert!(forever > max_stored_datetime());
        let stored = clamp_to_max_stored_datetime(forever);
        assert_eq!(stored, max_stored_datetime());
        assert!(is_max_datetime(&stored));
        assert!(is_max_datetime(&forever));
        // A four-digit year: sorts after every current time as text, too
        assert_eq!(stored.to_string(), "9999-12-31 23:59:59");

        let soon = PrimitiveDateTime::new(OffsetDateTime::now_utc().date(), OffsetDateTime::now_utc().time());
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
