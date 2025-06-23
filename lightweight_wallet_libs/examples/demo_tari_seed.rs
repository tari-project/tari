use lightweight_wallet_libs::key_management::{generate_seed_phrase, validate_seed_phrase, mnemonic_to_master_key};
use lightweight_wallet_libs::wallet::Wallet;

fn main() {
    println!("=== Tari CipherSeed Demo ===\n");
    
    // Generate 3 different Tari seed phrases
    for i in 1..=3 {
        println!("🔑 Generated Tari Seed Phrase #{}:", i);
        let phrase = generate_seed_phrase().unwrap();
        println!("   {}", phrase);
        
        // Validate it
        validate_seed_phrase(&phrase).unwrap();
        println!("   ✅ Valid Tari CipherSeed format");
        
        // Create wallet from it
        let wallet = Wallet::new_from_seed_phrase(&phrase, None).unwrap();
        println!("   ✅ Successfully created wallet");
        println!("   📅 Birthday: {}", wallet.birthday());
        
        // Export it back
        let exported = wallet.export_seed_phrase().unwrap();
        assert_eq!(phrase, exported);
        println!("   ✅ Successfully exported same phrase");
        println!();
    }
    
    println!("🎉 All Tari seed phrases working perfectly!");
    println!("\n📋 Key Points:");
    println!("   • Tari uses its own CipherSeed format (not BIP39)");
    println!("   • Generated phrases are 24 words from Tari word list");
    println!("   • They include encrypted metadata (birthday, version, etc.)");
    println!("   • Only Tari-generated phrases work with Tari wallets");
} 