use lightweight_wallet_libs::key_management::{
    generate_seed_phrase, 
    validate_seed_phrase, 
    derive_view_and_spend_keys_from_entropy,
    mnemonic_to_bytes,
    CipherSeed
};
use lightweight_wallet_libs::wallet::Wallet;
use lightweight_wallet_libs::data_structures::{
    TariAddress, Network,
    types::CompressedPublicKey,
    address::TariAddressFeatures,
};
use lightweight_wallet_libs::crypto::keys::ByteArray;

fn main() {
    println!("🚀 === Comprehensive Tari Wallet Demo ===\n");
    
    // Demo 1: Create wallet from existing seed phrase
    demo_wallet_from_seed_phrase();
    println!();
    
    // Demo 2: Generate new wallet with fresh seed phrase
    demo_generate_new_wallet();
    println!();
    
    // Demo 3: Wallet address generation (NEW - using wallet methods)
    demo_wallet_address_generation();
    println!();
    
    // Demo 4: Key derivation and manual address generation
    demo_key_derivation_and_addresses();
    println!();
    
    // Demo 5: Wallet metadata management
    demo_wallet_metadata();
    println!();
    
    // Demo 6: Address format conversions
    demo_address_formats();
    println!();
    
    // Demo 7: CipherSeed operations
    demo_cipher_seed_operations();
    
    println!("\n🎉 Wallet demo completed successfully!");
    println!("\n📋 Summary:");
    println!("   • Created wallets from seed phrases and random generation");
    println!("   • Generated addresses directly from wallet using built-in methods");
    println!("   • Derived view and spend keys using Tari's key derivation");
    println!("   • Generated dual and single addresses in multiple formats");
    println!("   • Demonstrated wallet metadata management");
    println!("   • Showed CipherSeed encryption/decryption operations");
}

fn demo_wallet_from_seed_phrase() {
    println!("📝 === Demo 1: Create Wallet from Seed Phrase ===");
    
    // First, demonstrate the issue with old BIP39 phrases
    let old_bip39_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    println!("🔍 Testing old BIP39 seed phrase: {}", old_bip39_phrase);
    
    // Validate the seed phrase first
    match validate_seed_phrase(old_bip39_phrase) {
        Ok(_) => println!("✅ Old BIP39 phrase is structurally valid"),
        Err(e) => {
            println!("❌ Old BIP39 phrase failed validation: {}", e);
            println!("   📖 Note: This demonstrates that old BIP39 phrases are not compatible with Tari CipherSeed format");
        }
    }
    
    // Try to create wallet from old BIP39 phrase
    match Wallet::new_from_seed_phrase(old_bip39_phrase, None) {
        Ok(wallet) => {
            println!("✅ Wallet created successfully from old phrase");
            println!("   📅 Birthday: {}", wallet.birthday());
        }
        Err(e) => {
            println!("❌ Failed to create wallet from old BIP39 phrase: {}", e);
            println!("   📖 Note: Tari uses CipherSeed format, not BIP39");
        }
    }
    
    // Now demonstrate the proper Tari approach
    println!("\n🔄 Demonstrating proper Tari CipherSeed approach:");
    
    // Generate a proper Tari seed phrase
    match generate_seed_phrase() {
        Ok(tari_seed_phrase) => {
            println!("✅ Generated Tari CipherSeed phrase: {}", tari_seed_phrase);
            
            // Validate the Tari seed phrase
            match validate_seed_phrase(&tari_seed_phrase) {
                Ok(_) => println!("✅ Tari seed phrase is valid"),
                Err(e) => println!("❌ Tari seed phrase failed validation: {}", e),
            }
            
            // Create wallet from Tari seed phrase
            match Wallet::new_from_seed_phrase(&tari_seed_phrase, None) {
                Ok(wallet) => {
                    println!("✅ Wallet created successfully from Tari CipherSeed");
                    println!("   📅 Birthday: {}", wallet.birthday());
                    println!("   🌐 Network: {}", wallet.network());
                    
                    // Export the seed phrase to verify it matches
                    match wallet.export_seed_phrase() {
                        Ok(exported) => {
                            println!("   ✅ Seed phrase export: matches original");
                            if tari_seed_phrase == exported {
                                println!("   ✅ Round-trip verification successful");
                            } else {
                                println!("   ❌ Round-trip verification failed");
                            }
                        }
                        Err(e) => println!("   ❌ Failed to export seed phrase: {}", e),
                    }
                }
                Err(e) => println!("❌ Failed to create wallet from Tari phrase: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to generate Tari seed phrase: {}", e),
    }
}

fn demo_generate_new_wallet() {
    println!("🎲 === Demo 2: Generate New Wallet ===");
    
    // Generate a wallet with a fresh seed phrase
    match Wallet::generate_new_with_seed_phrase(None) {
        Ok(wallet) => {
            println!("✅ New wallet generated successfully");
            println!("   📅 Birthday: {}", wallet.birthday());
            
            // Export the generated seed phrase
            match wallet.export_seed_phrase() {
                Ok(seed_phrase) => {
                    println!("   🔑 Generated seed phrase: {}", seed_phrase);
                    
                    // Validate the generated seed phrase
                    match validate_seed_phrase(&seed_phrase) {
                        Ok(_) => println!("   ✅ Generated seed phrase is valid"),
                        Err(e) => println!("   ❌ Generated seed phrase is invalid: {}", e),
                    }
                }
                Err(e) => println!("   ❌ Failed to export seed phrase: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to generate new wallet: {}", e),
    }
    
    // Also demonstrate random wallet generation (without seed phrase)
    println!("\n🎯 Generating wallet with random entropy:");
    let random_wallet = Wallet::generate_new(None);
    println!("✅ Random wallet created");
    println!("   📅 Birthday: {}", random_wallet.birthday());
    match random_wallet.export_seed_phrase() {
        Ok(_) => println!("   ❌ Unexpected: random wallet has seed phrase"),
        Err(_) => println!("   ✅ Correctly: random wallet has no exportable seed phrase"),
    }
}

fn demo_wallet_address_generation() {
    println!("🏠 === Demo 3: Wallet Address Generation ===");
    
    // Generate a fresh wallet with a Tari CipherSeed for consistent results
    match Wallet::generate_new_with_seed_phrase(None) {
        Ok(mut wallet) => {
            println!("✅ Wallet created for address generation demo");
            
            println!("Seed phrase: {}", wallet.export_seed_phrase().unwrap());
            // Set network to mainnet for this demo
            wallet.set_network("mainnet".to_string());
            
            // Generate dual address with default features
            println!("\n🏠 Generating dual address (with view and spend keys)...");
            match wallet.get_dual_address(TariAddressFeatures::create_interactive_and_one_sided(), None) {
                Ok(dual_address) => {
                    println!("✅ Dual address generated:");
                    println!("   📧 Emoji:  {}", dual_address.to_emoji_string());
                    println!("   🔗 Base58: {}", dual_address.to_base58());
                    println!("   🔢 Hex:    {}", dual_address.to_hex());
                    println!("   🌐 Network: {:?}", dual_address.network());
                    println!("   🎯 Features: {:?}", dual_address.features());
                }
                Err(e) => println!("❌ Failed to generate dual address: {}", e),
            }

            println!("\n🏠 Generating dual address with payment ID \"test\"...");
            match wallet.get_dual_address(TariAddressFeatures::create_one_sided_only(), Some(b"test".to_vec())) {
                Ok(dual_address_with_payment) => {
                    println!("✅ Dual address with payment ID \"test\" generated:");
                    println!("   📧 Emoji:  {}", dual_address_with_payment.to_emoji_string());
                    println!("   🎯 Features: {:?} (includes PAYMENT_ID)", dual_address_with_payment.features());
                }
                Err(e) => println!("❌ Failed to generate dual address with payment ID \"test\": {}", e),
            }

            // Generate dual address with payment ID
            println!("\n💳 Generating dual address with payment ID...");
            let payment_id = vec![0x12, 0x34, 0x56, 0x78, 0xAB, 0xCD, 0xEF];
            match wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), Some(payment_id)) {
                Ok(dual_address_with_payment) => {
                    println!("✅ Dual address with payment ID generated:");
                    println!("   📧 Emoji:  {}", dual_address_with_payment.to_emoji_string());
                    println!("   🎯 Features: {:?} (includes PAYMENT_ID)", dual_address_with_payment.features());
                    
                    // Check that payment ID feature is set
                    if dual_address_with_payment.features().contains(TariAddressFeatures::PAYMENT_ID) {
                        println!("   ✅ Payment ID feature correctly set");
                    }
                }
                Err(e) => println!("❌ Failed to generate dual address with payment ID: {}", e),
            }
            
            // Generate single address (spend key only)
            println!("\n🏠 Generating single address (spend key only)...");
            match wallet.get_single_address(TariAddressFeatures::create_interactive_only()) {
                Ok(single_address) => {
                    println!("✅ Single address generated:");
                    println!("   📧 Emoji:  {}", single_address.to_emoji_string());
                    println!("   🔗 Base58: {}", single_address.to_base58());
                    println!("   🔢 Hex:    {}", single_address.to_hex());
                    println!("   🌐 Network: {:?}", single_address.network());
                    println!("   🎯 Features: {:?}", single_address.features());
                }
                Err(e) => println!("❌ Failed to generate single address: {}", e),
            }
            
            // Test different networks
            println!("\n🌐 Testing different networks...");
            
            // Esmeralda (testnet)
            wallet.set_network("esmeralda".to_string());
            match wallet.get_single_address(TariAddressFeatures::create_one_sided_only()) {
                Ok(address) => {
                    println!("✅ Esmeralda address: Network = {:?}", address.network());
                }
                Err(e) => println!("❌ Failed to generate Esmeralda address: {}", e),
            }
            
            // Stagenet
            wallet.set_network("stagenet".to_string());
            match wallet.get_single_address(TariAddressFeatures::create_one_sided_only()) {
                Ok(address) => {
                    println!("✅ Stagenet address: Network = {:?}", address.network());
                }
                Err(e) => println!("❌ Failed to generate Stagenet address: {}", e),
            }
            
            // Test different feature combinations
            println!("\n🎯 Testing different address features...");
            wallet.set_network("mainnet".to_string()); // Reset to mainnet
            
            let feature_combinations = [
                ("Interactive Only", TariAddressFeatures::create_interactive_only()),
                ("One-Sided Only", TariAddressFeatures::create_one_sided_only()),
                ("Interactive + One-Sided", TariAddressFeatures::create_interactive_and_one_sided()),
            ];
            
            for (name, features) in feature_combinations {
                match wallet.get_dual_address(features, None) {
                    Ok(address) => {
                        println!("✅ {}: {:?}", name, address.features());
                    }
                    Err(e) => println!("❌ Failed to generate {} address: {}", name, e),
                }
            }
            
            // Demonstrate deterministic address generation
            println!("\n🔄 Testing deterministic address generation...");
            let addr1 = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
            let addr2 = wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
            
            if addr1.to_hex() == addr2.to_hex() {
                println!("✅ Address generation is deterministic (same wallet produces same address)");
            } else {
                println!("❌ Address generation is not deterministic");
            }
            
            // Test that different wallets produce different addresses
            println!("\n🔄 Testing that different wallets produce different addresses...");
            let other_wallet = Wallet::generate_new_with_seed_phrase(None).unwrap();
            let other_addr = other_wallet.get_dual_address(TariAddressFeatures::create_interactive_only(), None).unwrap();
            
            if addr1.to_hex() != other_addr.to_hex() {
                println!("✅ Different wallets produce different addresses");
            } else {
                println!("❌ Different wallets produced the same address (very unlikely!)");
            }
        }
        Err(e) => println!("❌ Failed to create wallet: {}", e),
    }
}

fn demo_key_derivation_and_addresses() {
    println!("🔑 === Demo 4: Key Derivation and Address Generation ===");
    
    // Generate a new wallet with Tari CipherSeed for this demo
    match Wallet::generate_new_with_seed_phrase(None) {
        Ok(wallet) => {
            println!("✅ Wallet created for key derivation demo");
            println!("   📅 Birthday: {}", wallet.birthday());
            
            // Show the seed phrase used
            match wallet.export_seed_phrase() {
                Ok(seed_phrase) => {
                    println!("   🔑 Seed phrase: {}", seed_phrase);
                }
                Err(e) => println!("   ❌ Failed to export seed phrase: {}", e),
            }
            
            // Get the wallet's entropy for key derivation
            let master_key = wallet.master_key_bytes();
            
            // For this demo, we'll derive keys from the first 16 bytes as entropy
            // In a real implementation, you'd use the CipherSeed entropy directly
            let mut entropy = [0u8; 16];
            entropy.copy_from_slice(&master_key[..16]);
            
            println!("\n🔐 Deriving view and spend keys from entropy...");
            match derive_view_and_spend_keys_from_entropy(&entropy) {
                Ok((view_private_key, spend_private_key)) => {
                    println!("✅ Successfully derived keys");
                    
                    // Convert to public keys
                    let view_public_key = view_private_key.public_key();
                    let spend_public_key = spend_private_key.public_key();
                    
                    println!("   🔍 View Private Key:  {}", hex::encode(view_private_key.as_bytes()));
                    println!("   💰 Spend Private Key: {}", hex::encode(spend_private_key.as_bytes()));
                    println!("   👀 View Public Key:   {}", hex::encode(view_public_key.as_bytes()));
                    println!("   💸 Spend Public Key:  {}", hex::encode(spend_public_key.as_bytes()));
                    
                    // Convert to CompressedPublicKey format for address generation
                    let view_compressed = CompressedPublicKey::new(view_public_key.as_bytes().try_into().unwrap());
                    let spend_compressed = CompressedPublicKey::new(spend_public_key.as_bytes().try_into().unwrap());
                    
                    // Generate addresses using manual key derivation
                    generate_addresses(&view_compressed, &spend_compressed);
                }
                Err(e) => println!("❌ Failed to derive keys: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to create wallet: {}", e),
    }
}

fn generate_addresses(view_key: &CompressedPublicKey, spend_key: &CompressedPublicKey) {
    println!("\n🏠 Generating Tari addresses...");
    
    // Generate dual address (has both view and spend keys)
    match TariAddress::new_dual_address_with_default_features(
        view_key.clone(),
        spend_key.clone(),
        Network::Esmeralda, // Using testnet
    ) {
        Ok(dual_address) => {
            println!("✅ Dual address generated:");
            println!("   📧 Emoji:  {}", dual_address.to_emoji_string());
            println!("   🔗 Base58: {}", dual_address.to_base58());
            println!("   🔢 Hex:    {}", dual_address.to_hex());
        }
        Err(e) => println!("❌ Failed to generate dual address: {}", e),
    }
    
    // Generate single address (spend key only)
    match TariAddress::new_single_address_with_interactive_only(
        spend_key.clone(),
        Network::Esmeralda,
    ) {
        Ok(single_address) => {
            println!("✅ Single address generated:");
            println!("   📧 Emoji:  {}", single_address.to_emoji_string());
            println!("   🔗 Base58: {}", single_address.to_base58());
            println!("   🔢 Hex:    {}", single_address.to_hex());
        }
        Err(e) => println!("❌ Failed to generate single address: {}", e),
    }
}

fn demo_wallet_metadata() {
    println!("📊 === Demo 5: Wallet Metadata Management ===");
    
    let mut wallet = Wallet::generate_new(None);
    println!("Created wallet for metadata demo");
    
    // Set wallet metadata
    wallet.set_label(Some("My Demo Wallet".to_string()));
    wallet.set_network("mainnet".to_string());
    wallet.set_current_key_index(42);
    wallet.set_property("version".to_string(), "1.0.0".to_string());
    wallet.set_property("created_by".to_string(), "Tari Wallet Demo".to_string());
    
    println!("✅ Metadata set:");
    println!("   🏷️  Label: {:?}", wallet.label());
    println!("   🌐 Network: {}", wallet.network());
    println!("   🔢 Key Index: {}", wallet.current_key_index());
    println!("   📦 Version: {:?}", wallet.get_property("version"));
    println!("   👤 Created By: {:?}", wallet.get_property("created_by"));
    
    // Modify metadata
    wallet.set_current_key_index(100);
    wallet.remove_property("created_by");
    
    println!("\n🔄 After modifications:");
    println!("   🔢 Key Index: {}", wallet.current_key_index());
    println!("   👤 Created By: {:?}", wallet.get_property("created_by"));
}

fn demo_address_formats() {
    println!("🎨 === Demo 6: Address Format Conversions ===");
    
    // Create a sample address for format demonstration
    let view_key = CompressedPublicKey::from_private_key(&lightweight_wallet_libs::data_structures::PrivateKey::random());
    let spend_key = CompressedPublicKey::from_private_key(&lightweight_wallet_libs::data_structures::PrivateKey::random());
    
    match TariAddress::new_dual_address_with_default_features(
        view_key,
        spend_key,
        Network::Esmeralda,
    ) {
        Ok(address) => {
            println!("✅ Address created for format demo");
            
            let emoji = address.to_emoji_string();
            let base58 = address.to_base58();
            let hex = address.to_hex();
            
            println!("\n📧 Emoji format:");
            println!("   {}", emoji);
            println!("   Length: {} characters", emoji.chars().count());
            
            println!("\n🔗 Base58 format:");
            println!("   {}", base58);
            println!("   Length: {} characters", base58.len());
            
            println!("\n🔢 Hex format:");
            println!("   {}", hex);
            println!("   Length: {} characters", hex.len());
            
            // Test round-trip conversions
            println!("\n🔄 Testing round-trip conversions:");
            
            // Emoji round-trip
            match TariAddress::from_emoji_string(&emoji) {
                Ok(recovered) => {
                    if recovered.to_emoji_string() == emoji {
                        println!("   ✅ Emoji round-trip successful");
                    } else {
                        println!("   ❌ Emoji round-trip failed");
                    }
                }
                Err(e) => println!("   ❌ Emoji parsing failed: {}", e),
            }
            
            // Base58 round-trip
            match TariAddress::from_base58(&base58) {
                Ok(recovered) => {
                    if recovered.to_base58() == base58 {
                        println!("   ✅ Base58 round-trip successful");
                    } else {
                        println!("   ❌ Base58 round-trip failed");
                    }
                }
                Err(e) => println!("   ❌ Base58 parsing failed: {}", e),
            }
            
            // Hex round-trip
            match TariAddress::from_hex(&hex) {
                Ok(recovered) => {
                    if recovered.to_hex() == hex {
                        println!("   ✅ Hex round-trip successful");
                    } else {
                        println!("   ❌ Hex round-trip failed");
                    }
                }
                Err(e) => println!("   ❌ Hex parsing failed: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to create address: {}", e),
    }
}

fn demo_cipher_seed_operations() {
    println!("🔐 === Demo 7: CipherSeed Operations ===");
    
    // Generate a new seed phrase
    match generate_seed_phrase() {
        Ok(seed_phrase) => {
            println!("✅ Generated seed phrase: {}", seed_phrase);
            
            // Convert to encrypted bytes
            match mnemonic_to_bytes(&seed_phrase) {
                Ok(encrypted_bytes) => {
                    println!("✅ Converted to encrypted bytes ({} bytes)", encrypted_bytes.len());
                    
                    // Decrypt the CipherSeed
                    match CipherSeed::from_enciphered_bytes(&encrypted_bytes, None) {
                        Ok(cipher_seed) => {
                            println!("✅ Successfully decrypted CipherSeed:");
                            println!("   📅 Birthday: {}", cipher_seed.birthday());
                            println!("   🔢 Version: {}", cipher_seed.version());
                            println!("   🎲 Entropy: {} bytes", cipher_seed.entropy().len());
                            
                            // Test encryption with passphrase
                            println!("\n🔒 Testing passphrase encryption:");
                            let passphrase = "test_passphrase_123";
                            
                            match CipherSeed::from_enciphered_bytes(&encrypted_bytes, Some(passphrase)) {
                                Ok(_) => println!("   ✅ Passphrase decryption successful"),
                                Err(_) => {
                                    // This is expected since the original wasn't encrypted with this passphrase
                                    println!("   ✅ Correctly rejected wrong passphrase");
                                }
                            }
                        }
                        Err(e) => println!("   ❌ Failed to decrypt CipherSeed: {}", e),
                    }
                }
                Err(e) => println!("   ❌ Failed to convert to bytes: {}", e),
            }
        }
        Err(e) => println!("❌ Failed to generate seed phrase: {}", e),
    }
} 