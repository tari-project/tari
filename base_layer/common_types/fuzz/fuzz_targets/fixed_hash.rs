#![no_main]

use std::convert::TryFrom;

use libfuzzer_sys::fuzz_target;
use tari_common_types::types::FixedHash;

fuzz_target!(|data: &[u8]| {
    let fixed_hash = FixedHash::try_from(data);
    match fixed_hash {
        Ok(f) => {
            assert_eq!(f.to_vec(), data);
        },
        Err(_) => {
            // As long as no panics
        },
    }
});
