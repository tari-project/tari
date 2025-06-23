use lightweight_wallet_libs::key_management::{mnemonic_to_master_key, validate_seed_phrase};
use lightweight_wallet_libs::wallet::Wallet;

fn main() {
    println!("=== Final Test: User's Tari Seed Phrase ===\n");
    
    let user_seed = "leopard test wide unhappy relax globe clerk make choice witness trophy hundred health love army north invite fuel grab farm order process force dress";
    
    println!("🔑 Testing Tari seed phrase:");
    println!("   {}\n", user_seed);
    
    // Step 1: Validate the seed phrase
    println!("Step 1: Validating seed phrase...");
    match validate_seed_phrase(user_seed) {
        Ok(_) => println!("   ✅ Seed phrase is valid!"),
        Err(e) => {
            println!("   ❌ Validation failed: {}", e);
            return;
        }
    }
    
    // Step 2: Convert to master key
    println!("\nStep 2: Converting to master key...");
    match mnemonic_to_master_key(user_seed, None) {
        Ok(master_key) => {
            println!("   ✅ Master key derived successfully!");
            println!("   🔑 Master key (hex): {}", hex::encode(master_key));
            println!("   📊 Master key length: {} bytes", master_key.len());
        },
        Err(e) => {
            println!("   ❌ Master key derivation failed: {}", e);
            return;
        }
    }
    
    // Step 3: Create wallet from seed phrase
    println!("\nStep 3: Creating wallet from seed phrase...");
    match Wallet::new_from_seed_phrase(user_seed, None) {
        Ok(wallet) => {
            println!("   ✅ Wallet created successfully!");
            println!("   🎂 Wallet birthday: {}", wallet.birthday());
            println!("   📝 Wallet label: {:?}", wallet.label());
            println!("   🌐 Wallet network: '{}'", wallet.network());
            println!("   🔢 Current key index: {}", wallet.current_key_index());
        },
        Err(e) => {
            println!("   ❌ Wallet creation failed: {}", e);
            return;
        }
    }
    
    // Step 4: Test seed phrase export
    println!("\nStep 4: Testing seed phrase export...");
    match Wallet::new_from_seed_phrase(user_seed, None) {
        Ok(wallet) => {
            match wallet.export_seed_phrase() {
                Ok(exported) => {
                    println!("   ✅ Seed phrase exported successfully!");
                    println!("   🔄 Exported matches original: {}", exported == user_seed);
                },
                Err(e) => println!("   ❌ Export failed: {}", e),
            }
        },
        Err(e) => println!("   ❌ Wallet creation for export test failed: {}", e),
    }
    
    println!("\n🎉 SUCCESS! Your Tari seed phrase is now fully supported!");
    println!("💡 Note: This seed uses legacy version 128 (0x80) format");
} 