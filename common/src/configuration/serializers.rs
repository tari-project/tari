//  Copyright 2022. The Tari Project
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

pub mod seconds {
    //! Helper module for serialising configuration variables from `Duration` to integers representing seconds and back.
    //! Use this converter by employing
    //! ```ignore
    //! use tari_common::configuration::serializers::seconds;
    //! ...
    //! #[serde(with="seconds")]
    //! pub my_var: Duration
    //! ```
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where D: Deserializer<'de> {
        Ok(Duration::from_secs(u64::deserialize(deserializer)?))
    }

    pub fn serialize<S>(duration: &Duration, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        s.serialize_u64(duration.as_secs())
    }
}

pub mod optional_seconds {
    //! Helper module for serialising configuration variables from `Duration` to integers representing seconds and back.
    //! Use this converter by employing
    //! ```ignore
    //! use tari_common::configuration::serializers::seconds;
    //! ...
    //! #[serde(with="optional_seconds")]
    //! pub my_var: Option<Duration>
    //! ```
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where D: Deserializer<'de> {
        match Option::<u64>::deserialize(deserializer)? {
            Some(d) => Ok(Some(Duration::from_secs(d))),
            None => Ok(None),
        }
    }

    pub fn serialize<S>(duration: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match duration {
            Some(d) => s.serialize_u64(d.as_secs()),
            None => s.serialize_none(),
        }
    }
}

pub mod seconds_nonzero {
    //! Same as [`seconds`](super::seconds), but rejects `0` at deserialization. Use it for intervals that drive a
    //! timer, where a zero value would spin or panic.
    //! ```ignore
    //! #[serde(with = "serializers::seconds_nonzero")]
    //! pub my_interval: Duration
    //! ```
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where D: Deserializer<'de> {
        let secs = u64::deserialize(deserializer)?;
        if secs == 0 {
            return Err(D::Error::custom("interval must be at least 1 second, got 0"));
        }
        Ok(Duration::from_secs(secs))
    }

    pub fn serialize<S>(duration: &Duration, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        s.serialize_u64(duration.as_secs())
    }
}

pub mod optional_seconds_nonzero {
    //! Same as [`optional_seconds`](super::optional_seconds), but rejects `Some(0)` at deserialization.
    //! ```ignore
    //! #[serde(with = "serializers::optional_seconds_nonzero")]
    //! pub my_interval: Option<Duration>
    //! ```
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where D: Deserializer<'de> {
        match Option::<u64>::deserialize(deserializer)? {
            Some(0) => Err(D::Error::custom("interval must be at least 1 second, got 0")),
            Some(d) => Ok(Some(Duration::from_secs(d))),
            None => Ok(None),
        }
    }

    pub fn serialize<S>(duration: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match duration {
            Some(d) => s.serialize_u64(d.as_secs()),
            None => s.serialize_none(),
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Test {
        #[serde(with = "super::seconds_nonzero")]
        interval: Duration,
    }

    #[derive(Deserialize)]
    struct TestOptional {
        #[serde(default, with = "super::optional_seconds_nonzero")]
        interval: Option<Duration>,
    }

    #[test]
    fn seconds_nonzero_rejects_zero() {
        assert!(toml::from_str::<Test>("interval = 0").is_err());
        let t = toml::from_str::<Test>("interval = 1").unwrap();
        assert_eq!(t.interval, Duration::from_secs(1));
    }

    #[test]
    fn optional_seconds_nonzero_rejects_zero() {
        assert!(toml::from_str::<TestOptional>("interval = 0").is_err());
        let t = toml::from_str::<TestOptional>("interval = 1").unwrap();
        assert_eq!(t.interval, Some(Duration::from_secs(1)));
        let t = toml::from_str::<TestOptional>("").unwrap();
        assert_eq!(t.interval, None);
    }
}
