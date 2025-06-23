use crate::data_structures::EncryptedData;
use borsh::{BorshDeserialize, BorshSerialize, to_vec, from_slice};
use serde_json;

fn serde_roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(value).unwrap();
    let de: T = serde_json::from_str(&json).unwrap();
    assert_eq!(value, &de);
    de
}

fn borsh_roundtrip<T>(value: &T) -> T
where
    T: BorshSerialize + BorshDeserialize + PartialEq + std::fmt::Debug,
{
    let bytes = to_vec(value).unwrap();
    let de = from_slice::<T>(&bytes).unwrap();
    assert_eq!(value, &de);
    de
}

#[test]
fn test_encrypted_data_serialization() {
    let ed = EncryptedData::default();
    serde_roundtrip(&ed);
    borsh_roundtrip(&ed);
}

#[test]
fn test_wallet_output_serialization() {
    let wo = crate::data_structures::wallet_output::LightweightWalletOutput::default();
    serde_roundtrip(&wo);
    borsh_roundtrip(&wo);
}

#[test]
fn test_transaction_output_serialization() {
    let to = crate::data_structures::transaction_output::LightweightTransactionOutput::default();
    serde_roundtrip(&to);
    borsh_roundtrip(&to);
}

#[test]
fn test_payment_id_serialization() {
    use crate::data_structures::payment_id::PaymentId;
    use primitive_types::U256;
    let ids = vec![
        PaymentId::Empty,
        PaymentId::U256 { value: U256::from(12345) },
        PaymentId::Open { data: vec![1, 2, 3] },
        PaymentId::AddressAndData { address: vec![4, 5], data: vec![6, 7] },
        PaymentId::TransactionInfo { tx_id: vec![8, 9], output_index: 42 },
        PaymentId::Raw { data: vec![10, 11, 12] },
    ];
    for id in ids {
        serde_roundtrip(&id);
        borsh_roundtrip(&id);
    }
} 