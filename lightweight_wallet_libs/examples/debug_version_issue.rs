use lightweight_wallet_libs::key_management::{generate_seed_phrase, validate_seed_phrase, mnemonic_to_master_key};
use lightweight_wallet_libs::wallet::Wallet;

fn main() {
    println!("=== Debugging Version Mismatch Issue ===\n");
    
    // Step 1: Generate a seed phrase
    println!("Step 1: Generating seed phrase...");
    let seed_phrase = match generate_seed_phrase() {
        Ok(phrase) => {
            println!("✅ Generated: {}", phrase);
            phrase
        },
        Err(e) => {
            println!("❌ Generation failed: {}", e);
            return;
        }
    };
    
    // Step 2: Validate the generated phrase
    println!("\nStep 2: Validating seed phrase...");
    match validate_seed_phrase(&seed_phrase) {
        Ok(_) => println!("✅ Validation successful"),
        Err(e) => {
            println!("❌ Validation failed: {}", e);
            return;
        }
    }
    
    // Step 3: Convert to master key
    println!("\nStep 3: Converting to master key...");
    match mnemonic_to_master_key(&seed_phrase, None) {
        Ok(key) => println!("✅ Master key conversion successful: {}...", key[..4].iter().map(|b| format!("{:02x}", b)).collect::<String>()),
        Err(e) => {
            println!("❌ Master key conversion failed: {}", e);
            return;
        }
    }
    
    // Step 4: Create wallet
    println!("\nStep 4: Creating wallet...");
    match Wallet::new_from_seed_phrase(&seed_phrase, None) {
        Ok(wallet) => {
            println!("✅ Wallet created successfully");
            println!("📅 Birthday: {}", wallet.birthday());
        },
        Err(e) => {
            println!("❌ Wallet creation failed: {}", e);
            return;
        }
    }
    
    // Step 5: Test multiple generation/validation cycles
    println!("\nStep 5: Testing multiple cycles...");
    for i in 1..=5 {
        match generate_seed_phrase() {
            Ok(phrase) => {
                print!("Cycle {}: ", i);
                match validate_seed_phrase(&phrase) {
                    Ok(_) => println!("✅ OK"),
                    Err(e) => {
                        println!("❌ FAILED: {}", e);
                        println!("Phrase: {}", phrase);
                        break;
                    }
                }
            },
            Err(e) => {
                println!("Cycle {}: ❌ Generation failed: {}", i, e);
                break;
            }
        }
    }
    
    println!("\n=== Debug Complete ===");
} 