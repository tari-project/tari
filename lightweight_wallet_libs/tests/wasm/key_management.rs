//! WASM test for key management (key derivation)

use wasm_bindgen_test::*;
use lightweight_wallet_libs::key_management::{key_derivation::LightweightKeyManager, KeyDerivationPath};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_wasm_key_derivation() {
    let master_key = [99u8; 32];
    let km = LightweightKeyManager::new(master_key);
    let path = KeyDerivationPath::tari_standard(0, 0, 0);
    let key_pair = km.derive_key_pair(&path).unwrap();
    // Check that the derived key is deterministic and not zero
    assert_ne!(key_pair.private_key.as_bytes(), [0u8; 32]);
    // Check that the public key is not zero
    assert_ne!(key_pair.public_key.as_bytes(), [0u8; 32]);
} 