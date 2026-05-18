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

use crate::configuration::ConfigList;

/// Supports deserialization from a sequence of strings or comma-delimited strings
pub type StringList = ConfigList<String>;

#[cfg(test)]
mod tests {
    use std::vec;

    use config::Config;
    use serde::Deserialize;

    use crate::configuration::StringList;

    #[derive(Deserialize)]
    struct Test {
        something: StringList,
    }

    #[test]
    fn with_capacity_test() {
        let new_str_lst = StringList::with_capacity(3);
        assert_eq!(new_str_lst.into_vec().capacity(), 3);
    }

    #[test]
    fn from_vec_string_list() {
        let vec_string = vec![String::from("Tari is cool!")];
        let string_lst = StringList::from(vec_string);
        assert_eq!(string_lst.into_vec(), vec![String::from("Tari is cool!")]);
    }

    #[test]
    fn as_ref_string_list() {
        let vec_string = vec![String::from("Tari")];
        let vec_as_ref: &[String] = vec_string.as_ref();
        let string_lst = StringList::from(vec![String::from("Tari")]);
        assert_eq!(string_lst.as_ref(), vec_as_ref);
    }

    #[test]
    fn into_iter_string_list() {
        let vec_string = vec![
            String::from("Tari"),
            String::from("Project"),
            String::from("let's mine it!"),
        ];
        let string_lst = StringList::from(vec_string);
        let mut res_iter = string_lst.into_iter();

        assert_eq!(Some(String::from("Tari")), res_iter.next());
        assert_eq!(Some(String::from("Project")), res_iter.next());
        assert_eq!(Some(String::from("let's mine it!")), res_iter.next());
        assert_eq!(None, res_iter.next());
    }

    #[test]
    fn it_deserializes_from_toml() {
        let config_str = r#"something = ["a","b","c"]"#;
        let test = toml::from_str::<Test>(config_str).unwrap();
        assert_eq!(test.something.into_vec(), vec!["a", "b", "c"]);
    }

    #[test]
    fn it_deserializes_from_config_comma_delimited() {
        let config = Config::builder()
            .set_override("something", "a, b, c,")
            .unwrap()
            .build()
            .unwrap();
        let test = config.try_deserialize::<Test>().unwrap();
        assert_eq!(test.something.into_vec(), vec!["a", "b", "c"]);
    }
}
