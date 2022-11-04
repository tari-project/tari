// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

// TODO: This struct will be available in tari_utilities soon. When it is there, it can be removed.
#[derive(Debug, Clone, Default)]
pub struct ByteCounter {
    count: usize,
}

impl ByteCounter {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get(&self) -> usize {
        self.count
    }
}

impl io::Write for ByteCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len();
        self.count += len;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;

    use super::*;

    #[test]
    fn write_test() {
        let mut byte_counter = ByteCounter::new();
        let buf = [0u8, 1u8, 2u8, 3u8];
        let new_count = byte_counter.write(&buf).unwrap();
        assert_eq!(byte_counter.get(), new_count);
    }

    #[test]
    fn flush_test() {
        let mut byte_counter = ByteCounter::new();
        let buf = [0u8, 1u8, 2u8, 3u8];
        let _count_bytes = byte_counter.write(&buf).unwrap();
        // test passes if the following method does not return an error
        byte_counter.flush().unwrap();
    }
}
