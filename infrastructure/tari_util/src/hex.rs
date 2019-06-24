use derive_error::Error;
use serde::Serializer;
use std::{
    fmt::{LowerHex, Write},
    num::ParseIntError,
};

/// Any object implementing this trait has the ability to represent itself as a hexadecimal string and convert from it.
pub trait Hex {
    type T;
    /// Try and convert the given hexadecimal string to the type. Any failures (incorrect  string length, non hex
    /// characters, etc) return a [KeyError](enum.KeyError.html) with an explanatory note.
    fn from_hex(hex: &str) -> Result<Self::T, HexError>;

    /// Return the hexadecimal string representation of the type
    fn to_hex(&self) -> String;
}

#[derive(Debug, Error)]
pub enum HexError {
    /// Only hexadecimal characters (0-9,a-f) are permitted
    InvalidCharacter(ParseIntError),
    /// Hex string lengths must be a multiple of 2
    LengthError,
    /// Invalid hex representation for the target type
    HexConversionError,
}

/// Encode the provided bytes into a hex string
pub fn to_hex<T>(bytes: &[T]) -> String
where T: LowerHex {
    let mut s = String::new();
    for byte in bytes {
        write!(&mut s, "{:02x}", byte).expect("Unable to write");
    }
    s
}

/// Encode the provided vector of bytes into a hex string
pub fn to_hex_multiple(bytearray: &[Vec<u8>]) -> Vec<String> {
    let mut result = Vec::new();
    for bytes in bytearray {
        result.push(to_hex(bytes))
    }
    result
}

/// Decode a hex string into bytes.
pub fn from_hex(hex_str: &str) -> Result<Vec<u8>, HexError> {
    let hex_trim = hex_str.trim();
    let hex_trim = if (hex_trim.len() >= 2) && (&hex_trim[..2] == "0x") {
        &hex_trim[2..]
    } else {
        hex_trim
    };
    if hex_trim.len() % 2 == 1 {
        return Err(HexError::LengthError);
    }
    let num_bytes = hex_trim.len() / 2;
    let mut result = vec![0u8; num_bytes];
    for i in 0..num_bytes {
        let val = u8::from_str_radix(&hex_trim[2 * i..2 * (i + 1)], 16);
        result[i] = match val {
            Ok(v) => v,
            Err(e) => return Err(HexError::InvalidCharacter(e)),
        }
    }
    Ok(result)
}

pub fn serialize_to_hex<S, T>(t: &T, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Hex,
{
    ser.serialize_str(&t.to_hex())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_to_hex() {
        assert_eq!(to_hex(&vec![0, 0, 0, 0]), "00000000");
        assert_eq!(to_hex(&vec![10, 11, 12, 13]), "0a0b0c0d");
        assert_eq!(to_hex(&vec![0, 0, 0, 255]), "000000ff");
    }

    #[test]
    fn test_from_hex() {
        assert_eq!(from_hex(&"00000000").unwrap(), vec![0, 0, 0, 0]);
        assert_eq!(from_hex(&"0a0b0c0d").unwrap(), vec![10, 11, 12, 13]);
        assert_eq!(from_hex(&" 0a0b0c0d  ").unwrap(), vec![10, 11, 12, 13]);
        assert_eq!(from_hex(&"000000ff").unwrap(), vec![0, 0, 0, 255]);
        assert_eq!(from_hex(&"0x800000ff").unwrap(), vec![128, 0, 0, 255]);
        assert!(from_hex(&"800").is_err()); // Odd number of bytes
        assert!(from_hex(&"8080gf").is_err()); // Invalid hex character g
    }

    #[test]
    fn length_error() {
        let result = from_hex(&"800");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            HexError::LengthError => (),
            _ => panic!(),
        }
        // Check that message is the doc message above
        assert_eq!(err.description(), "Hex string lengths must be a multiple of 2");
    }

    #[test]
    fn character_error() {
        let result = from_hex(&"1234567890ABCDEFG1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            HexError::InvalidCharacter(e) => println!("{:?}", e),
            _ => panic!(),
        }
        // Check that message is inherited from ParseIntError
        assert_eq!(err.description(), "invalid digit found in string");
    }
}
