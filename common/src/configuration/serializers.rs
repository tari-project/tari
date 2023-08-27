//  Copyright 2022. The Taiji Project
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
    //! use taiji_common::configuration::serializers::seconds;
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
    //! use taiji_common::configuration::serializers::seconds;
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
