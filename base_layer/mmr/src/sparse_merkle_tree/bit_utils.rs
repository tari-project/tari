use crate::sparse_merkle_tree::{NodeKey, SMTError};

/// Gets the bit at an offset from the most significant bit. Does NOT perform range checking
#[inline]
pub fn get_bit(data: &[u8], position: usize) -> usize {
    if (data[position / 8] as usize) & (1 << (8 - 1 - (position % 8))) > 0 {
        return 1;
    }
    0
}

// /// Sets the bit at an offset from the most significant bit
// #[inline]
// fn set_bit_at_from_msb(data: &mut [u8], position: usize) {
//     let mut n = data[position / 8] as usize;
//     n |= 1 << (8 - 1 - (position % 8));
//     data[position / 8] = n as u8;
// }
//
// #[inline]
// fn count_set_bits(data: &[u8]) -> usize {
//     let mut count = 0;
//     for i in 0..data.len() * 8 {
//         if get_bit_at_from_msb(data, i) == 1 {
//             count += 1;
//         }
//     }
//     count
// }
//
#[inline]
pub fn count_common_prefix(a: &NodeKey, b: &NodeKey) -> usize {
    let mut offset = 0;
    let n = a.len().min(b.len());
    let a = a.as_slice();
    let b = b.as_slice();
    while offset < n && a[offset] == b[offset] {
        offset += 1;
    }
    if offset == n {
        return offset * 8;
    }
    let mut i = 0;
    while get_bit(&a[offset..=offset], i) == get_bit(&b[offset..=offset], i) {
        i += 1;
    }
    offset * 8 + i
}

/// For branch nodes, the key is the first `height` bits of all descendant node keys. This function calculates the
/// branch key for a given key and height.
#[inline]
pub fn height_key(key: &NodeKey, height: usize) -> NodeKey {
    let mut result = NodeKey::default();
    // Keep the first `height` bits and ignore the rest
    let key = key.as_slice();
    let bytes = result.as_mut_slice();
    // First height/8 bytes are the same, so just copy
    bytes[0..height / 8].copy_from_slice(&key[0..height / 8]);
    // The height/8th byte is only partially copied, so mask the byte & 11100000, where the number of 1s is
    // height % 8
    bytes[height / 8] = key[height / 8] & !(0xff >> (height % 8));
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraverseDirection {
    Left,
    Right,
}

/// Checks whether the `child_key` would be a left or right child of the `parent_key` at the given height
pub fn traverse_direction(
    parent_height: usize,
    parent_key: &NodeKey,
    child_key: &NodeKey,
) -> Result<TraverseDirection, SMTError> {
    let common_prefix = count_common_prefix(parent_key, child_key);
    if common_prefix < parent_height {
        return Err(SMTError::InvalidChildKey {
            height: parent_height,
            child_key: child_key.clone(),
            parent_key: parent_key.clone(),
        });
    }

    match get_bit(child_key.as_slice(), parent_height) {
        0 => Ok(TraverseDirection::Left),
        1 => Ok(TraverseDirection::Right),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use super::*;
    use crate::sparse_merkle_tree::{bit_utils::count_common_prefix, NodeKey};

    #[test]
    fn test_common_prefix() {
        let a = NodeKey::from(b"abcdefgh12345678abcdefgh12345678");
        let b = NodeKey::from(b"abcdefgh12345678abcdefgh12345678");
        assert_eq!(count_common_prefix(&a, &b), 256);

        let b = NodeKey::from(b"abcDEFgh12345678abcdefgh12345678");
        // 'd' in binary is 01100100
        // 'D' in binary is 01000100
        assert_eq!(count_common_prefix(&a, &b), 3 * 8 + 2);

        let b = NodeKey::from(b"\xffbcdefgh12345678abcdefgh12345678");
        assert_eq!(count_common_prefix(&a, &b), 0);
    }

    #[test]
    fn traverse_directions() {
        let parent_key = NodeKey::try_from(b"\xffbcdefgh12345678abcdefgh12345678").unwrap();
        // 10111111 in hex is 0xBF
        let child_key = NodeKey::try_from(b"\xBFbcdefgh12345678abcdefgh12345678").unwrap();
        assert_eq!(
            traverse_direction(0, &parent_key, &child_key).unwrap(),
            TraverseDirection::Right
        );
        assert_eq!(
            traverse_direction(1, &parent_key, &child_key).unwrap(),
            TraverseDirection::Left
        );
        // 111... doesn't match 101.. to 2 places, so is an error
        let err = traverse_direction(2, &parent_key, &child_key);
        let expected_err = Err(SMTError::InvalidChildKey {
            height: 2,
            child_key,
            parent_key: parent_key.clone(),
        });
        assert_eq!(err, expected_err);
        // 11011111 in hex is 0xDF
        let child_key = NodeKey::try_from(b"\xDFbcdefgh12345678abcdefgh12345678").unwrap();
        assert_eq!(
            traverse_direction(0, &parent_key, &child_key).unwrap(),
            TraverseDirection::Right
        );
        // matches to 1 place, next is a 1, so is a right child
        assert_eq!(
            traverse_direction(1, &parent_key, &child_key).unwrap(),
            TraverseDirection::Right
        );
        // matches to 2 places, next is a 0, so is a left child
        assert_eq!(
            traverse_direction(2, &parent_key, &child_key).unwrap(),
            TraverseDirection::Left
        );

        let parent_key = NodeKey::try_from(b"abcdefgh\x082345678abcdefgh12345678").unwrap();
        let child_key = NodeKey::try_from(b"abcdefgh\x0A2345678abcdefgh12345678").unwrap();
        // 0x8 in binary is 00001000
        // 0xA in binary is 00001010
        // matches to 8*8 + 5 places, next is a 0, so is a left child
        assert_eq!(
            traverse_direction(69, &parent_key, &child_key).unwrap(),
            TraverseDirection::Left
        );
        // 0xC in binary is 00001100
        // matches to 8*8 + 5 places, next is a 1, so is a right child
        let child_key = NodeKey::try_from(b"abcdefgh\x0C2345678abcdefgh12345678").unwrap();
        assert_eq!(
            traverse_direction(69, &parent_key, &child_key).unwrap(),
            TraverseDirection::Right
        );

        // doesn't match to 70 places, so is an error
        let err = traverse_direction(71, &parent_key, &child_key);
        let expected_err: Result<TraverseDirection, _> = Err(SMTError::InvalidChildKey {
            height: 71,
            child_key,
            parent_key,
        });
        assert_eq!(err, expected_err);
    }

    #[test]
    fn height_keys() {
        let key = NodeKey::try_from(b"abcdefgh12345678abcdefgh12345678").unwrap();
        let hkey = height_key(&key, 0);
        assert_eq!(hkey.as_slice(), &[0u8; 32]);
        let hkey = height_key(&key, 3);
        let expected = NodeKey::from([
            96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(hkey, expected);
        let hkey = height_key(&key, 16);
        // 'a' in decimal is 97
        let expected = NodeKey::from([
            97, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(hkey, expected);
        let hkey = height_key(&key, 5 * 8 + 2);
        // 102 in binary is 01100110
        let expected = NodeKey::from([
            97, 98, 99, 100, 101, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(hkey, expected);
    }
}
