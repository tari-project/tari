use wasm_bindgen::prelude::*;
use crate::key_management::{key_derivation::LightweightKeyManager, KeyDerivationPath, KeyManager};

/// Derive a public key from a master key and BIP-44 path, returning it as a hex string.
#[wasm_bindgen]
pub fn derive_public_key_hex(
    master_key: &[u8],
    purpose: u32,
    coin_type: u32,
    account: u32,
    change: u32,
    address_index: u32,
) -> Result<String, JsValue> {
    if master_key.len() != 32 {
        return Err(JsValue::from_str("master_key must be 32 bytes"));
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(master_key);
    let km = LightweightKeyManager::new(key_bytes);
    let path = KeyDerivationPath::new(purpose, coin_type, account, change, address_index);
    let pk = km.derive_public_key(&path)
        .map_err(|e| JsValue::from_str(&format!("Key derivation error: {e}")))?;
    Ok(pk.to_hex())
} 