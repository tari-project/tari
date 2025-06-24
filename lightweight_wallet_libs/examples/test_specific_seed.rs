use lightweight_wallet_libs::key_management::{validate_seed_phrase, mnemonic_to_master_key};
use lightweight_wallet_libs::wallet::Wallet;

fn main() {
    println!("=== Testing Specific Seed Phrase ===\n");
    
    let user_seed = "leopard test wide unhappy relax globe clerk make choice witness trophy hundred health love army north invite fuel grab farm order process force dress";
    
    println!("Testing seed phrase:");
    println!("   {}\n", user_seed);
    
    // Step 1: Check word count
    let words: Vec<&str> = user_seed.split_whitespace().collect();
    println!("Step 1: Word count check");
    println!("   Word count: {}", words.len());
    if words.len() == 24 {
        println!("   ✅ Correct word count (24 words)");
    } else {
        println!("   ❌ Incorrect word count (expected 24, got {})", words.len());
    }
    
    // Step 2: Validate the seed phrase
    println!("\nStep 2: Validating seed phrase format...");
    match validate_seed_phrase(user_seed) {
        Ok(_) => println!("   ✅ Valid Tari CipherSeed format"),
        Err(e) => {
            println!("   ❌ Invalid Tari format: {}", e);
            println!("   💡 This appears to be a BIP39 seed phrase, not Tari CipherSeed");
        }
    }
    
    // Step 3: Try to convert to master key
    println!("\nStep 3: Attempting master key conversion...");
    match mnemonic_to_master_key(user_seed, None) {
        Ok(key) => {
            println!("   ✅ Master key conversion successful");
            println!("   🔑 Key preview: {}...", key[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());
        },
        Err(e) => {
            println!("   ❌ Master key conversion failed: {}", e);
        }
    }
    
    // Step 4: Try to create wallet
    println!("\nStep 4: Attempting wallet creation...");
    match Wallet::new_from_seed_phrase(user_seed, None) {
        Ok(wallet) => {
            println!("   ✅ Wallet created successfully");
            println!("   📅 Birthday: {}", wallet.birthday());
        },
        Err(e) => {
            println!("   ❌ Wallet creation failed: {}", e);
        }
    }
    
    let password_protected_seed = "scare cinnamon blast check harsh wisdom already tape senior guitar swim athlete leopard occur illegal connect weapon hood good jewel apple link able execute";
    let password = "test";
    // Step 5: Test with different passphrases
    println!("\nStep 5: Testing with passphrase...");
    match mnemonic_to_master_key(password_protected_seed, Some(password)) {
        Ok(key) => {
            println!("   ✅ With passphrase: {}...", key[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());
        },
        Err(e) => {
            println!("   ❌ With passphrase failed: {}", e);
        }
    }
    
    println!("\n=== Analysis Complete ===");
} 