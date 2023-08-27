#![no_main]

use std::convert::{TryFrom, TryInto};

use libfuzzer_sys::fuzz_target;
use prost::{bytes::Buf, Message};
use tari_core::{blocks::BlockHeader, proto::core::BlockHeader as ProtoBlockHeader};

fuzz_target!(|data: &[u8]| {
    match <ProtoBlockHeader as Message>::decode(data) {
        Ok(bh) => {
            match BlockHeader::try_from(bh) {
                Ok(h) => {
                    dbg!(h);
                    // Let's see what we have
                    todo!();
                },
                Err(e) => {
                    // As long as no panics
                },
            }
        },
        Err(e) => {
            // No problem
        },
    }
});
