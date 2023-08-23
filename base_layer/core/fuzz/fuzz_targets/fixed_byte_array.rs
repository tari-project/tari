#![no_main]

use borsh::{de::BorshDeserialize, BorshSerialize};
use libfuzzer_sys::fuzz_target;
use tari_core::proof_of_work::monero_rx::FixedByteArray;

fuzz_target!(|data: &[u8]| {
    let mut data2 = Vec::from(data);
    match FixedByteArray::deserialize(&mut data2.as_slice()) {
        Ok(fba) => {
            // dbg!(&fba);
            // This is fine as long as it doesn't panic
            let mut actual = vec![];
            fba.serialize(&mut actual).unwrap();
            // dbg!(&actual);
            // dbg!(data);
            assert_eq!(actual, &data[0..actual.len()]);
        },
        Err(_) => {
            // This is fine as long as it doesn't panic
        },
    }
});
