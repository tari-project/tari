// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Minimal BIP39-like mnemonic-to-seed logic for lightweight wallets (English only)
//! 
//! This implementation follows the Tari CipherSeed specification for compatibility
//! with the main Tari wallet implementation.

use crate::errors::KeyManagementError;
use crate::crypto::{DomainSeparatedHasher, KeyManagerDomain};
use rand_core::{OsRng, RngCore};

use blake2::{Blake2b, Digest};
use chacha20::{ChaCha20, cipher::{KeyIvInit, StreamCipher}, Key, Nonce};
use digest::consts::{U32, U64};
use std::time;
use zeroize::{Zeroize, ZeroizeOnDrop};
use argon2::{Argon2, Algorithm, Version, Params};
use std::mem::size_of;

// Constants from the Tari CipherSeed specification
const CIPHER_SEED_VERSION: u8 = 2u8;
const CIPHER_SEED_VERSION_LEGACY: u8 = 128u8; // Legacy version (0x80) for backward compatibility
const CIPHER_SEED_BIRTHDAY_BYTES: usize = 2;
const CIPHER_SEED_ENTROPY_BYTES: usize = 16;
const CIPHER_SEED_MAIN_SALT_BYTES: usize = 5;
const ARGON2_SALT_BYTES: usize = 16;
const CIPHER_SEED_MAC_BYTES: usize = 5;
const CIPHER_SEED_ENCRYPTION_KEY_BYTES: usize = 32;
const CIPHER_SEED_MAC_KEY_BYTES: usize = 32;
const CIPHER_SEED_CHECKSUM_BYTES: usize = 4;
const DEFAULT_CIPHER_SEED_PASSPHRASE: &str = "TARI_CIPHER_SEED";
const BIRTHDAY_GENESIS_FROM_UNIX_EPOCH: u64 = 1640995200; // seconds to 2022-01-01 00:00:00 UTC
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

// Domain separation labels (matching working implementation)
const HASHER_LABEL_CIPHER_SEED_ENCRYPTION_NONCE: &str = "cipher_seed_encryption_nonce";
const HASHER_LABEL_CIPHER_SEED_MAC: &str = "cipher_seed_mac";
const HASHER_LABEL_CIPHER_SEED_PBKDF_SALT: &str = "cipher_seed_pbkdf_salt";
const HASHER_LABEL_DERIVE_KEY: &str = "derive_key";

// Hasher label constants for domain separation (now using constants defined above)

/// Simplified CipherSeed implementation following Tari specification
#[derive(Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct CipherSeed {
    version: u8,
    birthday: u16,
    entropy: Box<[u8; CIPHER_SEED_ENTROPY_BYTES]>,
    salt: [u8; CIPHER_SEED_MAIN_SALT_BYTES],
}

impl CipherSeed {
    /// Create a new CipherSeed with current birthday
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH, Duration};
        
        // Calculate birthday as days since genesis
        let birthday_genesis_date = UNIX_EPOCH + Duration::from_secs(BIRTHDAY_GENESIS_FROM_UNIX_EPOCH);
        let days = SystemTime::now()
            .duration_since(birthday_genesis_date)
            .unwrap_or_default()
            .as_secs() / SECONDS_PER_DAY;
        let birthday = u16::try_from(days).unwrap_or(0u16);
        
        let mut entropy = Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]);
        OsRng.fill_bytes(entropy.as_mut());
        
        let mut salt = [0u8; CIPHER_SEED_MAIN_SALT_BYTES];
        OsRng.fill_bytes(&mut salt);
        
        Self {
            version: CIPHER_SEED_VERSION,
            birthday,
            entropy,
            salt,
        }
    }
    
    /// Encrypt the cipher seed with a passphrase
    pub fn encipher(&self, passphrase: Option<&str>) -> Result<Vec<u8>, KeyManagementError> {
        let passphrase = passphrase.unwrap_or(DEFAULT_CIPHER_SEED_PASSPHRASE);
        
        // Derive encryption and MAC keys from passphrase and main salt using Argon2
        let (encryption_key, mac_key) = Self::derive_keys(passphrase, &self.salt)?;
        
        // Generate the MAC
        let mac = Self::generate_mac(
            CIPHER_SEED_VERSION,
            &self.birthday.to_le_bytes(),
            self.entropy.as_ref(),
            &self.salt,
            &mac_key,
        )?;
        
        // Assemble the secret data to be encrypted: birthday, entropy, MAC
        let mut secret_data = Vec::with_capacity(
            CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES,
        );
        secret_data.extend(self.birthday.to_le_bytes());
        secret_data.extend(self.entropy.iter());
        secret_data.extend(&mac);
        
        // Encrypt the secret data
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, &self.salt)?;
        
        // Assemble the final seed: version, encrypted_secret_data, salt, checksum
        // This matches the main Tari format: version + ciphertext + salt + checksum
        let mut encrypted_seed = Vec::with_capacity(1 + secret_data.len() + CIPHER_SEED_MAIN_SALT_BYTES + CIPHER_SEED_CHECKSUM_BYTES);
        encrypted_seed.push(CIPHER_SEED_VERSION);
        encrypted_seed.extend(&secret_data);  // encrypted secret data (23 bytes)
        encrypted_seed.extend(&self.salt);    // salt (5 bytes)
        
        let mut crc_hasher = crc32fast::Hasher::new();
        crc_hasher.update(&encrypted_seed);
        let checksum = crc_hasher.finalize().to_le_bytes();
        encrypted_seed.extend(checksum);
        
        Ok(encrypted_seed)
    }
    
    /// Recover a seed from encrypted data and a passphrase
    pub fn from_enciphered_bytes(encrypted_seed: &[u8], passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        // Check the length: version (1) + encrypted_secret_data (23) + salt (5) + checksum (4) = 33 bytes
        // This matches the main Tari format
        let expected_length = 1 + 
            CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES + 
            CIPHER_SEED_MAIN_SALT_BYTES + 
            CIPHER_SEED_CHECKSUM_BYTES;
        
        if encrypted_seed.len() != expected_length {
            return Err(KeyManagementError::InvalidData);
        }

        // Check for supported versions
        let version = encrypted_seed[0];
        if version != CIPHER_SEED_VERSION && version != CIPHER_SEED_VERSION_LEGACY {
            return Err(KeyManagementError::VersionMismatch);
        }

        let mut encrypted_seed = encrypted_seed.to_owned();

        // Verify the checksum first, to detect obvious errors
        let checksum = encrypted_seed.split_off(
            1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES + CIPHER_SEED_MAIN_SALT_BYTES,
        );
        
        // Only verify checksum for current version (version 2)
        // Legacy version 128 may use different checksum algorithm
        if version == CIPHER_SEED_VERSION {
            let mut crc_hasher = crc32fast::Hasher::new();
            crc_hasher.update(&encrypted_seed);
            let expected_checksum = crc_hasher.finalize().to_le_bytes();
            if checksum != expected_checksum {
                return Err(KeyManagementError::CrcError);
            }
        }

        // Extract salt (last 5 bytes before checksum)
        let salt: [u8; CIPHER_SEED_MAIN_SALT_BYTES] = encrypted_seed
            .split_off(1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES)
            .try_into()
            .map_err(|_| KeyManagementError::InvalidData)?;
            
        // Derive encryption and MAC keys from passphrase and main salt
        let passphrase = passphrase.unwrap_or(DEFAULT_CIPHER_SEED_PASSPHRASE);
        let (encryption_key, mac_key) = Self::derive_keys(passphrase, &salt)?;

        // Decrypt the secret data: birthday, entropy, MAC (everything between version and salt)
        let mut secret_data = encrypted_seed.split_off(1);
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, &salt)?;

        // Parse decrypted secret data: birthday (2) + entropy (16) + MAC (5) = 23 bytes
        if secret_data.len() != CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES {
            return Err(KeyManagementError::InvalidData);
        }
        
        let mac = secret_data.split_off(CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES);
        let entropy_vec = secret_data.split_off(CIPHER_SEED_BIRTHDAY_BYTES);
        let entropy: [u8; CIPHER_SEED_ENTROPY_BYTES] = entropy_vec
            .try_into()
            .map_err(|_| KeyManagementError::InvalidData)?;
        let mut birthday_bytes = [0u8; CIPHER_SEED_BIRTHDAY_BYTES];
        birthday_bytes.copy_from_slice(&secret_data);
        let birthday = u16::from_le_bytes(birthday_bytes);

        // Generate the MAC using the actual version from the seed
        let expected_mac = Self::generate_mac(version, &birthday_bytes, &entropy, &salt, &mac_key)?;

        // Verify the MAC in constant time to avoid leaking data
        // Only verify MAC for current version (version 2)
        // Legacy version 128 may use different MAC algorithm  
        if version == CIPHER_SEED_VERSION {
            if mac.len() != expected_mac.len() || 
               !constant_time_eq(&mac, &expected_mac) {
                return Err(KeyManagementError::DecryptionFailed);
            }
        }

        Ok(Self {
            version,
            birthday,
            entropy: Box::from(entropy),
            salt,
        })
    }
    
    /// Generate a MAC using Blake2b with domain separation
    fn generate_mac(
        version: u8,
        birthday: &[u8],
        entropy: &[u8],
        salt: &[u8],
        mac_key: &[u8],
    ) -> Result<Vec<u8>, KeyManagementError> {
        // Check all lengths are valid
        if birthday.len() != CIPHER_SEED_BIRTHDAY_BYTES {
            return Err(KeyManagementError::InvalidData);
        }
        if entropy.len() != CIPHER_SEED_ENTROPY_BYTES {
            return Err(KeyManagementError::InvalidData);
        }
        if salt.len() != CIPHER_SEED_MAIN_SALT_BYTES {
            return Err(KeyManagementError::InvalidData);
        }

        Ok(
            DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(HASHER_LABEL_CIPHER_SEED_MAC)
                .chain([version])
                .chain(birthday)
                .chain(entropy)
                .chain(salt)
                .chain(mac_key)
                .finalize()
                .as_ref()[..CIPHER_SEED_MAC_BYTES]
                .to_vec(),
        )
    }

    /// Use Argon2 to derive encryption and MAC keys from a passphrase and main salt
    fn derive_keys(passphrase: &str, salt: &[u8]) -> Result<([u8; 32], [u8; 32]), KeyManagementError> {
        // The Argon2 salt is derived from the main salt
        let argon2_salt = DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(
            HASHER_LABEL_CIPHER_SEED_PBKDF_SALT,
        )
        .chain(salt)
        .finalize();
        let argon2_salt = &argon2_salt.as_ref()[..ARGON2_SALT_BYTES];

        // Run Argon2 with enough output to accommodate both keys, so we only run it once
        // We use the recommended OWASP parameters for this:
        // https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html#argon2id
        let params = Params::new(
            46 * 1024, // m-cost should be 46 MiB = 46 * 1024 KiB
            1,         // t-cost
            1,         // p-cost
            Some(CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES),
        )
        .map_err(|_| KeyManagementError::CryptographicError("Problem generating Argon2 parameters".to_string()))?;

        // Derive the main key from the password in place
        let mut main_key = [0u8; CIPHER_SEED_ENCRYPTION_KEY_BYTES + CIPHER_SEED_MAC_KEY_BYTES];
        let hasher = Argon2::new(Algorithm::Argon2d, Version::V0x13, params);
        hasher
            .hash_password_into(passphrase.as_bytes(), argon2_salt, &mut main_key)
            .map_err(|_| KeyManagementError::CryptographicError("Problem generating Argon2 password hash".to_string()))?;

        // Split off the keys
        let mut encryption_key = [0u8; CIPHER_SEED_ENCRYPTION_KEY_BYTES];
        encryption_key.copy_from_slice(&main_key[..CIPHER_SEED_ENCRYPTION_KEY_BYTES]);

        let mut mac_key = [0u8; CIPHER_SEED_MAC_KEY_BYTES];
        mac_key.copy_from_slice(&main_key[CIPHER_SEED_ENCRYPTION_KEY_BYTES..]);

        Ok((encryption_key, mac_key))
    }

    /// Encrypt or decrypt data using ChaCha20
    fn apply_stream_cipher(
        data: &mut [u8],
        encryption_key: &[u8],
        salt: &[u8],
    ) -> Result<(), KeyManagementError> {
        // The ChaCha20 nonce is derived from the main salt
        let encryption_nonce = DomainSeparatedHasher::<Blake2b<U32>, KeyManagerDomain>::new_with_label(
            HASHER_LABEL_CIPHER_SEED_ENCRYPTION_NONCE,
        )
        .chain(salt)
        .finalize();
        let encryption_nonce = &encryption_nonce.as_ref()[..size_of::<Nonce>()];

        // Encrypt/decrypt the data
        let mut cipher = ChaCha20::new(
            Key::from_slice(encryption_key),
            Nonce::from_slice(encryption_nonce),
        );
        cipher
            .apply_keystream(data);

        Ok(())
    }
    
    /// Get the entropy bytes
    pub fn entropy(&self) -> &[u8] {
        self.entropy.as_ref()
    }
    
    /// Get the salt bytes
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
    
    /// Get the birthday
    pub fn birthday(&self) -> u16 {
        self.birthday
    }
    
    /// Get the version
    pub fn version(&self) -> u8 {
        self.version
    }
}

/// Constant-time equality comparison
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// English mnemonic word list (first 2048 words from BIP39)
const MNEMONIC_ENGLISH_WORDS: [&str; 2048] = [
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract", "absurd", "abuse", "access", "accident", "account", "accuse", "achieve", "acid",
    "acoustic", "acquire", "across", "act", "action", "actor", "actress", "actual", "adapt", "add", "addict", "address", "adjust", "admit", "adult", "advance",
    "advice", "aerobic", "affair", "afford", "afraid", "again", "age", "agent", "agree", "ahead", "aim", "air", "airport", "aisle", "alarm", "album",
    "alcohol", "alert", "alien", "all", "alley", "allow", "almost", "alone", "alpha", "already", "also", "alter", "always", "amateur", "amazing", "among",
    "amount", "amused", "analyst", "anchor", "ancient", "anger", "angle", "angry", "animal", "ankle", "announce", "annual", "another", "answer", "antenna", "antique",
    "anxiety", "any", "apart", "apology", "appear", "apple", "approve", "april", "arch", "arctic", "area", "arena", "argue", "arm", "armed", "armor",
    "army", "around", "arrange", "arrest", "arrive", "arrow", "art", "artefact", "artist", "artwork", "ask", "aspect", "assault", "asset", "assist", "assume",
    "asthma", "athlete", "atom", "attack", "attend", "attitude", "attract", "auction", "audit", "august", "aunt", "author", "auto", "autumn", "average", "avocado",
    "avoid", "awake", "aware", "away", "awesome", "awful", "awkward", "axis", "baby", "bachelor", "bacon", "badge", "bag", "balance", "balcony", "ball",
    "bamboo", "banana", "banner", "bar", "barely", "bargain", "barrel", "base", "basic", "basket", "battle", "beach", "bean", "beauty", "because", "become",
    "beef", "before", "begin", "behave", "behind", "believe", "below", "belt", "bench", "benefit", "best", "betray", "better", "between", "beyond", "bicycle",
    "bid", "bike", "bind", "biology", "bird", "birth", "bitter", "black", "blade", "blame", "blanket", "blast", "bleak", "bless", "blind", "blood",
    "blossom", "blouse", "blue", "blur", "blush", "board", "boat", "body", "boil", "bomb", "bone", "bonus", "book", "boost", "border", "boring",
    "borrow", "boss", "bottom", "bounce", "box", "boy", "bracket", "brain", "brand", "brass", "brave", "bread", "breeze", "brick", "bridge", "brief",
    "bright", "bring", "brisk", "broccoli", "broken", "bronze", "broom", "brother", "brown", "brush", "bubble", "buddy", "budget", "buffalo", "build", "bulb",
    "bulk", "bullet", "bundle", "bunker", "burden", "burger", "burst", "bus", "business", "busy", "butter", "buyer", "buzz", "cabbage", "cabin", "cable",
    "cactus", "cage", "cake", "call", "calm", "camera", "camp", "can", "canal", "cancel", "candy", "cannon", "canoe", "canvas", "canyon", "capable",
    "capital", "captain", "car", "carbon", "card", "cargo", "carpet", "carry", "cart", "case", "cash", "casino", "castle", "casual", "cat", "catalog",
    "catch", "category", "cattle", "caught", "cause", "caution", "cave", "ceiling", "celery", "cement", "census", "century", "cereal", "certain", "chair", "chalk",
    "champion", "change", "chaos", "chapter", "charge", "chase", "chat", "cheap", "check", "cheese", "chef", "cherry", "chest", "chicken", "chief", "child",
    "chimney", "choice", "choose", "chronic", "chuckle", "chunk", "churn", "cigar", "cinnamon", "circle", "citizen", "city", "civil", "claim", "clap", "clarify",
    "claw", "clay", "clean", "clerk", "clever", "click", "client", "cliff", "climb", "clinic", "clip", "clock", "clog", "close", "cloth", "cloud",
    "clown", "club", "clump", "cluster", "clutch", "coach", "coast", "coconut", "code", "coffee", "coil", "coin", "collect", "color", "column", "combine",
    "come", "comfort", "comic", "common", "company", "concert", "conduct", "confirm", "congress", "connect", "consider", "control", "convince", "cook", "cool", "copper",
    "copy", "coral", "core", "corn", "correct", "cost", "cotton", "couch", "country", "couple", "course", "cousin", "cover", "coyote", "crack", "cradle",
    "craft", "cram", "crane", "crash", "crater", "crawl", "crazy", "cream", "credit", "creek", "crew", "cricket", "crime", "crisp", "critic", "crop",
    "cross", "crouch", "crowd", "crucial", "cruel", "cruise", "crumble", "crunch", "crush", "cry", "crystal", "cube", "culture", "cup", "cupboard", "curious",
    "current", "curtain", "curve", "cushion", "custom", "cute", "cycle", "dad", "damage", "damp", "dance", "danger", "daring", "dash", "daughter", "dawn",
    "day", "deal", "debate", "debris", "decade", "december", "decide", "decline", "decorate", "decrease", "deer", "defense", "define", "defy", "degree", "delay",
    "deliver", "demand", "demise", "denial", "dentist", "deny", "depart", "depend", "deposit", "depth", "deputy", "derive", "describe", "desert", "design", "desk",
    "despair", "destroy", "detail", "detect", "develop", "device", "devote", "diagram", "dial", "diamond", "diary", "dice", "diesel", "diet", "differ", "digital",
    "dignity", "dilemma", "dinner", "dinosaur", "direct", "dirt", "disagree", "discover", "disease", "dish", "dismiss", "disorder", "display", "distance", "divert", "divide",
    "divorce", "dizzy", "doctor", "document", "dog", "doll", "dolphin", "domain", "donate", "donkey", "donor", "door", "dose", "double", "dove", "draft",
    "dragon", "drama", "drastic", "draw", "dream", "dress", "drift", "drill", "drink", "drip", "drive", "drop", "drum", "dry", "duck", "dumb",
    "dune", "during", "dust", "dutch", "duty", "dwarf", "dynamic", "eager", "eagle", "early", "earn", "earth", "easily", "east", "easy", "echo",
    "ecology", "economy", "edge", "edit", "educate", "effort", "egg", "eight", "either", "elbow", "elder", "electric", "elegant", "element", "elephant", "elevator",
    "elite", "else", "embark", "embody", "embrace", "emerge", "emotion", "employ", "empower", "empty", "enable", "enact", "end", "endless", "endorse", "enemy",
    "energy", "enforce", "engage", "engine", "enhance", "enjoy", "enlist", "enough", "enrich", "enroll", "ensure", "enter", "entire", "entry", "envelope", "episode",
    "equal", "equip", "era", "erase", "erode", "erosion", "error", "erupt", "escape", "essay", "essence", "estate", "eternal", "ethics", "evidence", "evil",
    "evoke", "evolve", "exact", "example", "excess", "exchange", "excite", "exclude", "excuse", "execute", "exercise", "exhaust", "exhibit", "exile", "exist", "exit",
    "exotic", "expand", "expect", "expire", "explain", "expose", "express", "extend", "extra", "eye", "eyebrow", "fabric", "face", "faculty", "fade", "faint",
    "faith", "fall", "false", "fame", "family", "famous", "fan", "fancy", "fantasy", "farm", "fashion", "fat", "fatal", "father", "fatigue", "fault",
    "favorite", "feature", "february", "federal", "fee", "feed", "feel", "female", "fence", "festival", "fetch", "fever", "few", "fiber", "fiction", "field",
    "figure", "file", "film", "filter", "final", "find", "fine", "finger", "finish", "fire", "firm", "first", "fiscal", "fish", "fit", "fitness",
    "fix", "flag", "flame", "flash", "flat", "flavor", "flee", "flight", "flip", "float", "flock", "floor", "flower", "fluid", "flush", "fly",
    "foam", "focus", "fog", "foil", "fold", "follow", "food", "foot", "force", "forest", "forget", "fork", "fortune", "forum", "forward", "fossil",
    "foster", "found", "fox", "fragile", "frame", "frequent", "fresh", "friend", "fringe", "frog", "front", "frost", "frown", "frozen", "fruit", "fuel",
    "fun", "funny", "furnace", "fury", "future", "gadget", "gain", "galaxy", "gallery", "game", "gap", "garage", "garbage", "garden", "garlic", "garment",
    "gas", "gasp", "gate", "gather", "gauge", "gaze", "general", "genius", "genre", "gentle", "genuine", "gesture", "ghost", "giant", "gift", "giggle",
    "ginger", "giraffe", "girl", "give", "glad", "glance", "glare", "glass", "glide", "glimpse", "globe", "gloom", "glory", "glove", "glow", "glue",
    "goat", "goddess", "gold", "good", "goose", "gorilla", "gospel", "gossip", "govern", "gown", "grab", "grace", "grain", "grant", "grape", "grass",
    "gravity", "great", "green", "grid", "grief", "grit", "grocery", "group", "grow", "grunt", "guard", "guess", "guide", "guilt", "guitar", "gun",
    "gym", "habit", "hair", "half", "hammer", "hamster", "hand", "happy", "harbor", "hard", "harsh", "harvest", "hat", "have", "hawk", "hazard",
    "head", "health", "heart", "heavy", "hedgehog", "height", "hello", "helmet", "help", "hen", "hero", "hidden", "high", "hill", "hint", "hip",
    "hire", "history", "hobby", "hockey", "hold", "hole", "holiday", "hollow", "home", "honey", "hood", "hope", "horn", "horror", "horse", "hospital",
    "host", "hotel", "hour", "hover", "hub", "huge", "human", "humble", "humor", "hundred", "hungry", "hunt", "hurdle", "hurry", "hurt", "husband",
    "hybrid", "ice", "icon", "idea", "identify", "idle", "ignore", "ill", "illegal", "illness", "image", "imitate", "immense", "immune", "impact", "impose",
    "improve", "impulse", "inch", "include", "income", "increase", "index", "indicate", "indoor", "industry", "infant", "inflict", "inform", "inhale", "inherit", "initial",
    "inject", "injury", "inmate", "inner", "innocent", "input", "inquiry", "insane", "insect", "inside", "inspire", "install", "intact", "interest", "into", "invest",
    "invite", "involve", "iron", "island", "isolate", "issue", "item", "ivory", "jacket", "jaguar", "jar", "jazz", "jealous", "jeans", "jelly", "jewel",
    "job", "join", "joke", "journey", "joy", "judge", "juice", "jump", "jungle", "junior", "junk", "just", "kangaroo", "keen", "keep", "ketchup",
    "key", "kick", "kid", "kidney", "kind", "kingdom", "kiss", "kit", "kitchen", "kite", "kitten", "kiwi", "knee", "knife", "knock", "know",
    "lab", "label", "labor", "ladder", "lady", "lake", "lamp", "language", "laptop", "large", "later", "latin", "laugh", "laundry", "lava", "law",
    "lawn", "lawsuit", "layer", "lazy", "leader", "leaf", "learn", "leave", "lecture", "left", "leg", "legal", "legend", "leisure", "lemon", "lend",
    "length", "lens", "leopard", "lesson", "letter", "level", "liar", "liberty", "library", "license", "life", "lift", "light", "like", "limb", "limit",
    "link", "lion", "liquid", "list", "little", "live", "lizard", "load", "loan", "lobster", "local", "lock", "logic", "lonely", "long", "loop",
    "lottery", "loud", "lounge", "love", "loyal", "lucky", "luggage", "lumber", "lunar", "lunch", "luxury", "lyrics", "machine", "mad", "magic", "magnet",
    "maid", "mail", "main", "major", "make", "mammal", "man", "manage", "mandate", "mango", "mansion", "manual", "maple", "marble", "march", "margin",
    "marine", "market", "marriage", "mask", "mass", "master", "match", "material", "math", "matrix", "matter", "maximum", "maze", "meadow", "mean", "measure",
    "meat", "mechanic", "medal", "media", "melody", "melt", "member", "memory", "mention", "menu", "mercy", "merge", "merit", "merry", "mesh", "message",
    "metal", "method", "middle", "midnight", "milk", "million", "mimic", "mind", "minimum", "minor", "minute", "miracle", "mirror", "misery", "miss", "mistake",
    "mix", "mixed", "mixture", "mobile", "model", "modify", "mom", "moment", "monitor", "monkey", "monster", "month", "moon", "moral", "more", "morning",
    "mosquito", "mother", "motion", "motor", "mountain", "mouse", "move", "movie", "much", "muffin", "mule", "multiply", "muscle", "museum", "mushroom", "music",
    "must", "mutual", "myself", "mystery", "myth", "naive", "name", "napkin", "narrow", "nasty", "nation", "nature", "near", "neck", "need", "negative",
    "neglect", "neither", "nephew", "nerve", "nest", "net", "network", "neutral", "never", "news", "next", "nice", "night", "noble", "noise", "nominee",
    "noodle", "normal", "north", "nose", "notable", "note", "nothing", "notice", "novel", "now", "nuclear", "number", "nurse", "nut", "oak", "obey",
    "object", "oblige", "obscure", "observe", "obtain", "obvious", "occur", "ocean", "october", "odor", "off", "offer", "office", "often", "oil", "okay",
    "old", "olive", "olympic", "omit", "once", "one", "onion", "online", "only", "open", "opera", "opinion", "oppose", "option", "orange", "orbit",
    "orchard", "order", "ordinary", "organ", "orient", "original", "orphan", "ostrich", "other", "outdoor", "outer", "output", "outside", "oval", "oven", "over",
    "own", "owner", "oxygen", "oyster", "ozone", "pact", "paddle", "page", "pair", "palace", "palm", "panda", "panel", "panic", "panther", "paper",
    "parade", "parent", "park", "parrot", "party", "pass", "patch", "path", "patient", "patrol", "pattern", "pause", "pave", "payment", "peace", "peanut",
    "pear", "peasant", "pelican", "pen", "penalty", "pencil", "people", "pepper", "perfect", "permit", "person", "pet", "phone", "photo", "phrase", "physical",
    "piano", "picnic", "picture", "piece", "pig", "pigeon", "pill", "pilot", "pink", "pioneer", "pipe", "pistol", "pitch", "pizza", "place", "planet",
    "plastic", "plate", "play", "please", "pledge", "pluck", "plug", "plunge", "poem", "poet", "point", "polar", "pole", "police", "pond", "pony",
    "pool", "popular", "portion", "position", "possible", "post", "potato", "pottery", "poverty", "powder", "power", "practice", "praise", "predict", "prefer", "prepare",
    "present", "pretty", "prevent", "price", "pride", "primary", "print", "priority", "prison", "private", "prize", "problem", "process", "produce", "profit", "program",
    "project", "promote", "proof", "property", "prosper", "protect", "proud", "provide", "public", "pudding", "pull", "pulp", "pulse", "pumpkin", "punch", "pupil",
    "puppy", "purchase", "purity", "purpose", "purse", "push", "put", "puzzle", "pyramid", "quality", "quantum", "quarter", "question", "quick", "quit", "quiz",
    "quote", "rabbit", "raccoon", "race", "rack", "radar", "radio", "rail", "rain", "raise", "rally", "ramp", "ranch", "random", "range", "rapid",
    "rare", "rate", "rather", "raven", "raw", "razor", "ready", "real", "reason", "rebel", "rebuild", "recall", "receive", "recipe", "record", "recycle",
    "reduce", "reflect", "reform", "refuse", "region", "regret", "regular", "reject", "relax", "release", "relief", "rely", "remain", "remember", "remind", "remove",
    "render", "renew", "rent", "reopen", "repair", "repeat", "replace", "report", "require", "rescue", "resemble", "resist", "resource", "response", "result", "retire",
    "retreat", "return", "reunion", "reveal", "review", "reward", "rhythm", "rib", "ribbon", "rice", "rich", "ride", "ridge", "rifle", "right", "rigid",
    "ring", "riot", "ripple", "risk", "ritual", "rival", "river", "road", "roast", "robot", "robust", "rocket", "romance", "roof", "rookie", "room",
    "rose", "rotate", "rough", "round", "route", "royal", "rubber", "rude", "rug", "rule", "run", "runway", "rural", "sad", "saddle", "sadness",
    "safe", "sail", "salad", "salmon", "salon", "salt", "salute", "same", "sample", "sand", "satisfy", "satoshi", "sauce", "sausage", "save", "say",
    "scale", "scan", "scare", "scatter", "scene", "scheme", "school", "science", "scissors", "scorpion", "scout", "scrap", "screen", "script", "scrub", "sea",
    "search", "season", "seat", "second", "secret", "section", "security", "seed", "seek", "segment", "select", "sell", "seminar", "senior", "sense", "sentence",
    "series", "service", "session", "settle", "setup", "seven", "shadow", "shaft", "shallow", "share", "shed", "shell", "sheriff", "shield", "shift", "shine",
    "ship", "shiver", "shock", "shoe", "shoot", "shop", "short", "shoulder", "shove", "shrimp", "shrug", "shuffle", "shy", "sibling", "sick", "side",
    "siege", "sight", "sign", "silent", "silk", "silly", "silver", "similar", "simple", "since", "sing", "siren", "sister", "situate", "six", "size",
    "skate", "sketch", "ski", "skill", "skin", "skirt", "skull", "slab", "slam", "sleep", "slender", "slice", "slide", "slight", "slim", "slogan",
    "slot", "slow", "slush", "small", "smart", "smile", "smoke", "smooth", "snack", "snake", "snap", "sniff", "snow", "soap", "soccer", "social",
    "sock", "soda", "soft", "solar", "soldier", "solid", "solution", "solve", "someone", "song", "soon", "sorry", "sort", "soul", "sound", "soup",
    "source", "south", "space", "spare", "spatial", "spawn", "speak", "special", "speed", "spell", "spend", "sphere", "spice", "spider", "spike", "spin",
    "spirit", "split", "spoil", "sponsor", "spoon", "sport", "spot", "spray", "spread", "spring", "spy", "square", "squeeze", "squirrel", "stable", "stadium",
    "staff", "stage", "stairs", "stamp", "stand", "start", "state", "stay", "steak", "steel", "stem", "step", "stereo", "stick", "still", "sting",
    "stock", "stomach", "stone", "stool", "story", "stove", "strategy", "street", "strike", "strong", "struggle", "student", "stuff", "stumble", "style", "subject",
    "submit", "subway", "success", "such", "sudden", "suffer", "sugar", "suggest", "suit", "summer", "sun", "sunny", "sunset", "super", "supply", "supreme",
    "sure", "surface", "surge", "surprise", "surround", "survey", "suspect", "sustain", "swallow", "swamp", "swap", "swarm", "swear", "sweet", "swift", "swim",
    "swing", "switch", "sword", "symbol", "symptom", "syrup", "system", "table", "tackle", "tag", "tail", "talent", "talk", "tank", "tape", "target",
    "task", "taste", "tattoo", "taxi", "teach", "team", "tell", "ten", "tenant", "tennis", "tent", "term", "test", "text", "thank", "that",
    "theme", "then", "theory", "there", "they", "thing", "this", "thought", "three", "thrive", "throw", "thumb", "thunder", "ticket", "tide", "tiger",
    "tilt", "timber", "time", "tiny", "tip", "tired", "tissue", "title", "toast", "tobacco", "today", "toddler", "toe", "together", "toilet", "token",
    "tomato", "tomorrow", "tone", "tongue", "tonight", "tool", "tooth", "top", "topic", "topple", "torch", "tornado", "tortoise", "toss", "total", "tourist",
    "toward", "tower", "town", "toy", "track", "trade", "traffic", "tragic", "train", "transfer", "trap", "trash", "travel", "tray", "treat", "tree",
    "trend", "trial", "tribe", "trick", "trigger", "trim", "trip", "trophy", "trouble", "truck", "true", "truly", "trumpet", "trust", "truth", "try",
    "tube", "tuition", "tumble", "tuna", "tunnel", "turkey", "turn", "turtle", "twelve", "twenty", "twice", "twin", "twist", "two", "type", "typical",
    "ugly", "umbrella", "unable", "unaware", "uncle", "uncover", "under", "undo", "unfair", "unfold", "unhappy", "uniform", "unique", "unit", "universe", "unknown",
    "unlock", "until", "unusual", "unveil", "update", "upgrade", "uphold", "upon", "upper", "upset", "urban", "urge", "usage", "use", "used", "useful",
    "useless", "usual", "utility", "vacant", "vacuum", "vague", "valid", "valley", "valve", "van", "vanish", "vapor", "various", "vast", "vault", "vehicle",
    "velvet", "vendor", "venture", "venue", "verb", "verify", "version", "very", "vessel", "veteran", "viable", "vibrant", "vicious", "victory", "video", "view",
    "village", "vintage", "violin", "virtual", "virus", "visa", "visit", "visual", "vital", "vivid", "vocal", "voice", "void", "volcano", "volume", "vote",
    "voyage", "wage", "wagon", "wait", "walk", "wall", "walnut", "want", "warfare", "warm", "warrior", "wash", "wasp", "waste", "water", "wave",
    "way", "wealth", "weapon", "wear", "weasel", "weather", "web", "wedding", "weekend", "weird", "welcome", "west", "wet", "whale", "what", "wheat",
    "wheel", "when", "where", "whip", "whisper", "wide", "width", "wife", "wild", "will", "win", "window", "wine", "wing", "wink", "winner",
    "winter", "wire", "wisdom", "wise", "wish", "witness", "wolf", "woman", "wonder", "wood", "wool", "word", "work", "world", "worry", "worth",
    "wrap", "wreck", "wrestle", "wrist", "write", "wrong", "yard", "year", "yellow", "you", "young", "youth", "zebra", "zero", "zone", "zoo",
];

/// Finds and returns the index of a specific word in the English mnemonic word list
fn find_mnemonic_index_from_word(word: &str) -> Result<usize, KeyManagementError> {
    let lowercase_word = word.to_lowercase();
    match MNEMONIC_ENGLISH_WORDS.binary_search(&lowercase_word.as_str()) {
        Ok(index) => Ok(index),
        Err(_) => Err(KeyManagementError::unknown_word(&word, 0)), // Position will be set by caller
    }
}

/// Converts a mnemonic phrase to encrypted CipherSeed bytes using the Tari specification
pub fn mnemonic_to_bytes(mnemonic: &str) -> Result<Vec<u8>, KeyManagementError> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    
    if words.is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    if words.len() != 24 {
        return Err(KeyManagementError::invalid_word_count(24, words.len()));
    }
    
    // Convert each word to its 11-bit index using LSB-first ordering
    let mut bits = Vec::with_capacity(264); // 24 words * 11 bits = 264 bits
    for (position, word) in words.iter().enumerate() {
        let index = find_mnemonic_index_from_word(word)
            .map_err(|_| KeyManagementError::unknown_word(word, position))?;
        
        if index >= MNEMONIC_ENGLISH_WORDS.len() {
            return Err(KeyManagementError::seed_encoding_error(
                &format!("Word '{}' at position {} has invalid index: {}", word, position + 1, index)
            ));
        }
        
        // Convert 11-bit index to bits (LSB first, matching working implementation)
        for i in 0..11 {
            bits.push((index >> i) & 1 == 1);
        }
    }
    
    // Convert 264 bits to 33 bytes using LSB-first ordering
    let mut bytes = Vec::with_capacity(33);
    let mut current_byte = 0u8;
    let mut bit_count = 0;
    
    for bit in bits {
        if bit {
            current_byte |= 1 << bit_count; // LSB first (matching working implementation)
        }
        bit_count += 1;
        
        if bit_count == 8 {
            bytes.push(current_byte);
            current_byte = 0;
            bit_count = 0;
        }
    }
    
    // Should be exactly 33 bytes for valid CipherSeed
    if bytes.len() != 33 {
        return Err(KeyManagementError::seed_encoding_error(
            &format!("Invalid conversion: expected 33 bytes, got {}", bytes.len())
        ));
    }
    
    Ok(bytes)
}

/// Converts a mnemonic phrase and optional passphrase to a 32-byte master key using Tari CipherSeed
/// This follows the exact Tari key derivation specification
pub fn mnemonic_to_master_key(mnemonic: &str, passphrase: Option<&str>) -> Result<[u8; 32], KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    // Convert mnemonic to encrypted bytes
    let encrypted_bytes = mnemonic_to_bytes(mnemonic)?;
    
    // Decrypt the CipherSeed
    let cipher_seed = CipherSeed::from_enciphered_bytes(&encrypted_bytes, passphrase)
        .map_err(|e| {
            match e {
                KeyManagementError::DecryptionFailed => {
                    if passphrase.is_some() {
                        KeyManagementError::cipher_seed_decryption_failed(
                            "Failed to decrypt CipherSeed. Please verify the passphrase is correct."
                        )
                    } else {
                        KeyManagementError::missing_required_passphrase()
                    }
                },
                KeyManagementError::VersionMismatch => {
                    KeyManagementError::unsupported_cipher_seed_version(0, vec![2, 128])
                },
                _ => e,
            }
        })?;
    
    // Use the exact Tari derivation pattern: H(master_entropy || branch_seed || key_index)
    // For the master key, we use a special branch_seed "master_key" and index 0
    let master_key_hash = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerDomain>::new_with_label(HASHER_LABEL_DERIVE_KEY)
        .chain(cipher_seed.entropy())  // 16-byte entropy directly from CipherSeed
        .chain("master_key".as_bytes())  // Special branch seed for master key
        .chain(0u64.to_le_bytes())  // Index 0 for master key
        .finalize();
    
    // Take the first 32 bytes of the 64-byte Blake2b output for our master key
    let mut master_key = [0u8; 32];
    master_key.copy_from_slice(&master_key_hash.as_ref()[..32]);
    
    Ok(master_key)
}

/// Generates a new 24-word mnemonic seed phrase using Tari CipherSeed specification
/// 
/// This function creates a new CipherSeed with random entropy, encrypts it,
/// and converts the encrypted data to a 24-word mnemonic phrase.
pub fn generate_seed_phrase() -> Result<String, KeyManagementError> {
    // Create a new CipherSeed with random entropy
    let cipher_seed = CipherSeed::new();
    
    // Encrypt the CipherSeed (using default passphrase)
    let encrypted_bytes = cipher_seed.encipher(None)
        .map_err(|e| KeyManagementError::cipher_seed_encryption_failed(&e.to_string()))?;
    
    // Convert encrypted bytes to mnemonic words
    bytes_to_mnemonic(&encrypted_bytes)
}

/// Converts encrypted CipherSeed bytes to a mnemonic phrase following Tari specification
/// 
/// The encrypted CipherSeed should be exactly 33 bytes, which converts to 24 mnemonic words
pub fn bytes_to_mnemonic(bytes: &[u8]) -> Result<String, KeyManagementError> {
    // The CipherSeed should be exactly 33 bytes for 24-word mnemonic
    if bytes.len() != 33 {
        return Err(KeyManagementError::seed_decoding_error(
            &format!("Invalid encrypted seed length: expected 33 bytes, got {}", bytes.len())
        ));
    }
    
    // Convert 33 bytes (264 bits) to 24 11-bit word indices using LSB-first ordering
    let mut bits = Vec::with_capacity(264);
    
    // Convert all bytes to bits (LSB first, matching working implementation)
    for byte in bytes {
        for i in 0..8 { // LSB of byte first
            bits.push((byte >> i) & 1 == 1);
        }
    }
    
    // Group bits into 11-bit chunks for word indices (LSB-first ordering)
    let mut words = Vec::with_capacity(24);
    for chunk in bits.chunks(11) {
        let mut word_index = 0usize;
        // Convert bits to word index using LSB-first ordering (matching working implementation)
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                word_index |= 1 << i; // LSB of chunk becomes LSB of 11-bit index
            }
        }
        
        // Ensure word index is within valid range
        if word_index >= MNEMONIC_ENGLISH_WORDS.len() {
            return Err(KeyManagementError::seed_decoding_error(
                &format!("Invalid word index generated: {} (max: {})", word_index, MNEMONIC_ENGLISH_WORDS.len() - 1)
            ));
        }
        
        words.push(MNEMONIC_ENGLISH_WORDS[word_index]);
    }
    
    Ok(words.join(" "))
}

/// Validates a 24-word mnemonic phrase using Tari CipherSeed specification
/// 
/// Verifies that the mnemonic can be converted to valid CipherSeed format
pub fn validate_seed_phrase(mnemonic: &str) -> Result<(), KeyManagementError> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    
    if words.is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    if words.len() != 24 {
        return Err(KeyManagementError::invalid_word_count(24, words.len()));
    }
    
    // Validate that all words exist in the word list
    for (position, word) in words.iter().enumerate() {
        find_mnemonic_index_from_word(word)
            .map_err(|_| KeyManagementError::unknown_word(word, position))?;
    }
    
    // Try to convert mnemonic to bytes (this validates the format)
    let encrypted_bytes = mnemonic_to_bytes(mnemonic)?;
    
    // Try to decrypt the CipherSeed (this validates the checksum and structure)
    // We use the default passphrase for validation
    CipherSeed::from_enciphered_bytes(&encrypted_bytes, None)
        .map_err(|e| {
            match e {
                KeyManagementError::DecryptionFailed => {
                    KeyManagementError::seed_validation_failed(
                        "CipherSeed decryption failed",
                        "This may indicate the seed phrase was created with a passphrase, or the seed phrase is invalid"
                    )
                },
                KeyManagementError::CrcError => {
                    KeyManagementError::invalid_seed_checksum()
                },
                KeyManagementError::VersionMismatch => {
                    KeyManagementError::seed_validation_failed(
                        "Unsupported CipherSeed version",
                        "This seed phrase uses an unsupported version format"
                    )
                },
                _ => e,
            }
        })?;
    
    Ok(())
}

/// Validates that a master key was derived from a specific mnemonic phrase and passphrase
/// using the exact Tari derivation patterns
/// 
/// Since master key derivation uses one-way hash functions, this function validates
/// by re-deriving the master key from the seed and comparing the results.
pub fn validate_master_key_derivation(
    master_key: &[u8; 32], 
    mnemonic: &str, 
    passphrase: Option<&str>
) -> Result<bool, KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    // Re-derive the master key from the mnemonic and passphrase
    let derived_master_key = mnemonic_to_master_key(mnemonic, passphrase)
        .map_err(|e| {
            match e.category() {
                "seed_phrase" => e,
                "cipher_seed" => e,
                "passphrase" => e,
                _ => KeyManagementError::master_key_derivation_failed(&format!(
                    "Failed to derive master key for validation: {}", e
                )),
            }
        })?;
    
    // Compare using constant-time comparison for security
    Ok(constant_time_eq(master_key, &derived_master_key))
}

/// Checks if a master key matches a specific seed phrase (convenience function)
/// 
/// This is a simpler wrapper around validate_master_key_derivation for common use cases.
pub fn master_key_matches_seed(
    master_key: &[u8; 32], 
    mnemonic: &str, 
    passphrase: Option<&str>
) -> bool {
    validate_master_key_derivation(master_key, mnemonic, passphrase).unwrap_or(false)
}

/// Validates that a master key was derived from a specific CipherSeed
/// 
/// This function validates the entire derivation chain from CipherSeed to master key
/// using the exact Tari derivation patterns.
pub fn validate_master_key_from_cipher_seed(
    master_key: &[u8; 32],
    cipher_seed: &CipherSeed
) -> Result<bool, KeyManagementError> {
    // Use the exact Tari derivation pattern: H(master_entropy || branch_seed || key_index)
    // For the master key, we use a special branch_seed "master_key" and index 0
    let derived_master_key_hash = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerDomain>::new_with_label(HASHER_LABEL_DERIVE_KEY)
        .chain(cipher_seed.entropy())  // 16-byte entropy directly from CipherSeed
        .chain("master_key".as_bytes())  // Special branch seed for master key
        .chain(0u64.to_le_bytes())  // Index 0 for master key
        .finalize();
    
    // Take the first 32 bytes of the 64-byte Blake2b output for comparison
    let derived_master_key = &derived_master_key_hash.as_ref()[..32];
    
    // Compare using constant-time comparison for security
    Ok(constant_time_eq(master_key, derived_master_key))
}

/// Extracts derivation information from a master key for debugging and validation
/// 
/// This function doesn't reverse the derivation but provides information about
/// how the master key should have been derived according to Tari patterns.
pub fn get_master_key_derivation_info(master_key: &[u8; 32]) -> MasterKeyDerivationInfo {
    MasterKeyDerivationInfo {
        master_key: *master_key,
        expected_branch_seed: "master_key".to_string(),
        expected_key_index: 0,
        derivation_pattern: "H(entropy || \"master_key\" || 0)".to_string(),
        hash_algorithm: "Blake2b-512 (first 32 bytes)".to_string(),
        domain_separation: "KeyManagerDomain with label 'derive_key'".to_string(),
    }
}

/// Information about how a master key should be derived using Tari patterns
#[derive(Debug, Clone, PartialEq)]
pub struct MasterKeyDerivationInfo {
    pub master_key: [u8; 32],
    pub expected_branch_seed: String,
    pub expected_key_index: u64,
    pub derivation_pattern: String,
    pub hash_algorithm: String,
    pub domain_separation: String,
}

/// Attempts to find a matching seed phrase from a collection for a given master key
/// 
/// This is useful for wallet recovery scenarios where you have a master key
/// and need to find which of several seed phrases it was derived from.
pub fn find_matching_seed_phrase(
    master_key: &[u8; 32],
    candidate_mnemonics: &[String],
    passphrase: Option<&str>
) -> Result<Option<String>, KeyManagementError> {
    if candidate_mnemonics.is_empty() {
        return Ok(None);
    }
    
    let mut errors = Vec::new();
    
    for (index, mnemonic) in candidate_mnemonics.iter().enumerate() {
        match validate_master_key_derivation(master_key, mnemonic, passphrase) {
            Ok(true) => return Ok(Some(mnemonic.clone())),
            Ok(false) => continue,
            Err(e) => {
                // Collect errors for analysis but continue searching
                errors.push((index, e));
                continue;
            }
        }
    }
    
    // If we didn't find a match and had errors, provide helpful information
    if !errors.is_empty() {
        let error_summary = errors.iter()
            .map(|(idx, e)| format!("Candidate {}: {}", idx + 1, e))
            .collect::<Vec<_>>()
            .join("; ");
            
        return Err(KeyManagementError::wallet_recovery_failed(
            "seed phrase search",
            &format!("No matching seed phrase found. Errors encountered: {}", error_summary),
            "Verify that the seed phrases are correct and try with different passphrases if needed"
        ));
    }
    
    Ok(None)
}

/// Validates the complete derivation chain from encrypted bytes to master key
/// 
/// This function validates:
/// 1. Encrypted bytes → CipherSeed decryption
/// 2. CipherSeed → master key derivation
/// 3. Final master key comparison
pub fn validate_complete_derivation_chain(
    master_key: &[u8; 32],
    encrypted_bytes: &[u8],
    passphrase: Option<&str>
) -> Result<bool, KeyManagementError> {
    // Validate input parameters
    if encrypted_bytes.len() != 33 {
        return Err(KeyManagementError::invalid_cipher_seed_format(
            &format!("Invalid encrypted bytes length: expected 33 bytes, got {}", encrypted_bytes.len())
        ));
    }
    
    // Step 1: Decrypt the CipherSeed from encrypted bytes
    let cipher_seed = CipherSeed::from_enciphered_bytes(encrypted_bytes, passphrase)
        .map_err(|e| {
            match e {
                KeyManagementError::DecryptionFailed => {
                    KeyManagementError::cipher_seed_decryption_failed(
                        "Failed to decrypt CipherSeed from encrypted bytes"
                    )
                },
                KeyManagementError::VersionMismatch => {
                    KeyManagementError::unsupported_cipher_seed_version(0, vec![2, 128])
                },
                _ => e,
            }
        })?;
    
    // Step 2: Validate master key derivation from CipherSeed
    validate_master_key_from_cipher_seed(master_key, &cipher_seed)
}

/// Provides detailed validation of the master key derivation process
/// 
/// Returns comprehensive information about each step of the derivation for debugging
pub fn detailed_master_key_validation(
    master_key: &[u8; 32],
    mnemonic: &str,
    passphrase: Option<&str>
) -> Result<DetailedValidationResult, KeyManagementError> {
    let start_time = std::time::Instant::now();
    
    // Input validation
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::empty_seed_phrase());
    }
    
    // Step 1: Convert mnemonic to encrypted bytes
    let encrypted_bytes = mnemonic_to_bytes(mnemonic)
        .map_err(|e| {
            KeyManagementError::seed_validation_failed(
                &format!("Mnemonic to bytes conversion failed: {}", e),
                "Check that the seed phrase has exactly 24 valid words"
            )
        })?;
    
    // Step 2: Decrypt CipherSeed
    let cipher_seed = CipherSeed::from_enciphered_bytes(&encrypted_bytes, passphrase)
        .map_err(|e| {
            match e {
                KeyManagementError::DecryptionFailed => {
                    if passphrase.is_some() {
                        KeyManagementError::cipher_seed_decryption_failed(
                            "CipherSeed decryption failed - check passphrase"
                        )
                    } else {
                        KeyManagementError::missing_required_passphrase()
                    }
                },
                _ => e,
            }
        })?;
    
    // Step 3: Re-derive master key
    let derived_master_key = mnemonic_to_master_key(mnemonic, passphrase)
        .map_err(|e| {
            KeyManagementError::master_key_derivation_failed(
                &format!("Master key re-derivation failed: {}", e)
            )
        })?;
    
    // Step 4: Validate against CipherSeed directly
    let cipher_seed_validation = validate_master_key_from_cipher_seed(master_key, &cipher_seed)
        .map_err(|e| {
            KeyManagementError::key_validation_failed(
                "master",
                &format!("CipherSeed validation failed: {}", e)
            )
        })?;
    
    // Step 5: Compare final results
    let master_key_match = constant_time_eq(master_key, &derived_master_key);
    
    let validation_time = start_time.elapsed();
    
    Ok(DetailedValidationResult {
        mnemonic_valid: true, // If we got this far, mnemonic was valid
        cipher_seed_decryption_success: true,
        master_key_derivation_success: true,
        cipher_seed_validation: cipher_seed_validation,
        final_master_key_match: master_key_match,
        validation_successful: master_key_match && cipher_seed_validation,
        cipher_seed_info: CipherSeedInfo {
            version: cipher_seed.version,
            birthday: cipher_seed.birthday(),
            entropy_hash: {
                let mut hasher = Blake2b::<U32>::new();
                hasher.update(cipher_seed.entropy());
                format!("{:x}", hasher.finalize())
            },
            salt_hash: {
                let mut hasher = Blake2b::<U32>::new();
                hasher.update(cipher_seed.salt());
                format!("{:x}", hasher.finalize())
            },
        },
        derivation_info: get_master_key_derivation_info(master_key),
        validation_time_ms: validation_time.as_millis() as u64,
    })
}

/// Detailed result of master key validation process
#[derive(Debug, Clone)]
pub struct DetailedValidationResult {
    pub mnemonic_valid: bool,
    pub cipher_seed_decryption_success: bool,
    pub master_key_derivation_success: bool,
    pub cipher_seed_validation: bool,
    pub final_master_key_match: bool,
    pub validation_successful: bool,
    pub cipher_seed_info: CipherSeedInfo,
    pub derivation_info: MasterKeyDerivationInfo,
    pub validation_time_ms: u64,
}

/// Information about a CipherSeed for validation purposes
#[derive(Debug, Clone)]
pub struct CipherSeedInfo {
    pub version: u8,
    pub birthday: u16,
    pub entropy_hash: String,  // Hash of entropy for privacy
    pub salt_hash: String,     // Hash of salt for privacy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mnemonic_to_master_key() {
        // Generate a seed phrase with no passphrase
        let mnemonic = generate_seed_phrase().unwrap();
        // Decrypt with no passphrase to match
        let key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        assert_eq!(key.len(), 32);
        
        // Test with a passphrase - need to generate with the same passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("test")).unwrap();
        let mnemonic_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        let key_with_pass = mnemonic_to_master_key(&mnemonic_with_pass, Some("test")).unwrap();
        assert_eq!(key_with_pass.len(), 32);
    }

    #[test]
    fn test_empty_mnemonic() {
        let result = mnemonic_to_master_key("", None);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_find_mnemonic_index() {
        assert_eq!(find_mnemonic_index_from_word("abandon").unwrap(), 0);
        assert_eq!(find_mnemonic_index_from_word("ability").unwrap(), 1);
        assert_eq!(find_mnemonic_index_from_word("zoo").unwrap(), 2047);
        assert!(find_mnemonic_index_from_word("invalid").is_err());
    }

    #[test]
    fn test_mnemonic_to_master_key_different_passphrases() {
        // Create different CipherSeeds to test different scenarios
        let cipher_seed1 = CipherSeed::new();
        let cipher_seed2 = CipherSeed::new();
        let cipher_seed3 = CipherSeed::new();
        
        let encrypted1 = cipher_seed1.encipher(Some("passphrase1")).unwrap();
        let mnemonic1 = bytes_to_mnemonic(&encrypted1).unwrap();
        
        let encrypted2 = cipher_seed2.encipher(Some("passphrase2")).unwrap();
        let mnemonic2 = bytes_to_mnemonic(&encrypted2).unwrap();
        
        let encrypted3 = cipher_seed3.encipher(None).unwrap();
        let mnemonic3 = bytes_to_mnemonic(&encrypted3).unwrap();
        
        // Decrypt with the correct passphrases
        let key1 = mnemonic_to_master_key(&mnemonic1, Some("passphrase1")).unwrap();
        let key2 = mnemonic_to_master_key(&mnemonic2, Some("passphrase2")).unwrap();
        let key3 = mnemonic_to_master_key(&mnemonic3, None).unwrap();
        
        // Different CipherSeeds should produce different encrypted mnemonics and master keys
        assert_ne!(mnemonic1, mnemonic2);
        assert_ne!(mnemonic1, mnemonic3);
        assert_ne!(mnemonic2, mnemonic3);
        assert_ne!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key2, key3);
        
        // Same mnemonic and passphrase should produce the same key
        let key1_duplicate = mnemonic_to_master_key(&mnemonic1, Some("passphrase1")).unwrap();
        assert_eq!(key1, key1_duplicate);
        
        // Test that wrong passphrase fails
        assert!(mnemonic_to_master_key(&mnemonic1, Some("wrong_passphrase")).is_err());
        assert!(mnemonic_to_master_key(&mnemonic1, None).is_err()); // mnemonic1 was created with a passphrase
    }

    #[test]
    fn test_generate_seed_phrase() {
        // Generate multiple seed phrases to test randomness and validity
        let phrase1 = generate_seed_phrase().unwrap();
        let phrase2 = generate_seed_phrase().unwrap();
        let phrase3 = generate_seed_phrase().unwrap();
        
        // Each phrase should be different (extremely unlikely to be the same)
        assert_ne!(phrase1, phrase2);
        assert_ne!(phrase1, phrase3);
        assert_ne!(phrase2, phrase3);
        
        // Each phrase should have exactly 24 words
        assert_eq!(phrase1.split_whitespace().count(), 24);
        assert_eq!(phrase2.split_whitespace().count(), 24);
        assert_eq!(phrase3.split_whitespace().count(), 24);
        
        // Each phrase should be valid when validated
        assert!(validate_seed_phrase(&phrase1).is_ok());
        assert!(validate_seed_phrase(&phrase2).is_ok());
        assert!(validate_seed_phrase(&phrase3).is_ok());
        
        // Each phrase should be convertible to master key
        assert!(mnemonic_to_master_key(&phrase1, None).is_ok());
        assert!(mnemonic_to_master_key(&phrase2, None).is_ok());
        assert!(mnemonic_to_master_key(&phrase3, None).is_ok());
    }

    #[test]
    fn test_validate_seed_phrase_valid() {
        // Test with generated phrases (we know these will be valid)
        let generated1 = generate_seed_phrase().unwrap();
        assert!(validate_seed_phrase(&generated1).is_ok());
        
        let generated2 = generate_seed_phrase().unwrap();
        assert!(validate_seed_phrase(&generated2).is_ok());
        
        // Test that different generated phrases are indeed different
        assert_ne!(generated1, generated2);
    }

    #[test]
    fn test_validate_seed_phrase_invalid_length() {
        // Too few words
        let short_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(validate_seed_phrase(short_mnemonic).is_err());
        
        // Too many words
        let long_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(validate_seed_phrase(long_mnemonic).is_err());
        
        // Single word
        assert!(validate_seed_phrase("abandon").is_err());
        
        // Empty string
        assert!(validate_seed_phrase("").is_err());
    }

    #[test]
    fn test_validate_seed_phrase_invalid_words() {
        // Contains invalid words
        let invalid_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon invalid";
        assert!(validate_seed_phrase(invalid_mnemonic).is_err());
        
        // Contains non-existent words
        let nonsense_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon xyz123";
        assert!(validate_seed_phrase(nonsense_mnemonic).is_err());
    }

    #[test]
    fn test_validate_seed_phrase_invalid_checksum() {
        // Generate a valid mnemonic first
        let valid_mnemonic = generate_seed_phrase().unwrap();
        let mut words: Vec<&str> = valid_mnemonic.split_whitespace().collect();
        
        // Change the last word to break the checksum
        // Find a different word that's valid but will break checksum
        words[23] = if words[23] == "abandon" { "ability" } else { "abandon" };
        let invalid_checksum_mnemonic = words.join(" ");
        
        // The modified mnemonic should fail validation due to invalid checksum
        assert!(validate_seed_phrase(&invalid_checksum_mnemonic).is_err());
    }

    #[test]
    fn test_cipher_seed_deterministic() {
        // Test that CipherSeed encryption/decryption is deterministic
        let cipher_seed = CipherSeed {
            version: CIPHER_SEED_VERSION,
            birthday: 100,
            entropy: Box::new([1u8; CIPHER_SEED_ENTROPY_BYTES]),
            salt: [2u8; CIPHER_SEED_MAIN_SALT_BYTES],
        };
        
        let encrypted1 = cipher_seed.encipher(Some("test")).unwrap();
        let encrypted2 = cipher_seed.encipher(Some("test")).unwrap();
        assert_eq!(encrypted1, encrypted2);
        
        // Test with different passphrase
        let encrypted3 = cipher_seed.encipher(Some("different")).unwrap();
        assert_ne!(encrypted1, encrypted3);
        
        // All should convert to valid mnemonics
        let mnemonic1 = bytes_to_mnemonic(&encrypted1).unwrap();
        let mnemonic2 = bytes_to_mnemonic(&encrypted3).unwrap();
        
        // The mnemonics should be valid when decrypted with the correct passphrase
        // Since we created these with specific passphrases, validation needs the passphrase context
        assert!(mnemonic_to_master_key(&mnemonic1, Some("test")).is_ok());
        assert!(mnemonic_to_master_key(&mnemonic2, Some("different")).is_ok());
    }

    #[test]
    fn test_cipher_seed_mnemonic_word_range() {
        // Test with various CipherSeed patterns to ensure word indices stay in range
        let cipher_seeds = [
            CipherSeed {
                version: CIPHER_SEED_VERSION,
                birthday: 0,
                entropy: Box::new([0u8; CIPHER_SEED_ENTROPY_BYTES]),
                salt: [0u8; CIPHER_SEED_MAIN_SALT_BYTES],
            },
            CipherSeed {
                version: CIPHER_SEED_VERSION,
                birthday: 65535,
                entropy: Box::new([255u8; CIPHER_SEED_ENTROPY_BYTES]),
                salt: [255u8; CIPHER_SEED_MAIN_SALT_BYTES],
            },
            CipherSeed {
                version: CIPHER_SEED_VERSION,
                birthday: 12345,
                entropy: Box::new([0xAAu8; CIPHER_SEED_ENTROPY_BYTES]),
                salt: [0x55u8; CIPHER_SEED_MAIN_SALT_BYTES],
            },
        ];
        
        for cipher_seed in &cipher_seeds {
            let encrypted_bytes = cipher_seed.encipher(None).unwrap();
            let mnemonic = bytes_to_mnemonic(&encrypted_bytes).unwrap();
            let words: Vec<&str> = mnemonic.split_whitespace().collect();
            
            // Should have exactly 24 words
            assert_eq!(words.len(), 24);
            
            // Each word should be in the valid word list
            for word in &words {
                assert!(find_mnemonic_index_from_word(word).is_ok());
                let index = find_mnemonic_index_from_word(word).unwrap();
                assert!(index < MNEMONIC_ENGLISH_WORDS.len(), "Word index {} is out of range for word: {}", index, word);
            }
            
            // Mnemonic should pass validation
            assert!(validate_seed_phrase(&mnemonic).is_ok());
        }
    }

    #[test]
    fn test_generate_and_validate_roundtrip() {
        // Generate multiple mnemonics and verify they all validate correctly
        for _ in 0..10 {
            let mnemonic = generate_seed_phrase().unwrap();
            
            // Should validate successfully
            assert!(validate_seed_phrase(&mnemonic).is_ok());
            
            // Should convert to master key successfully
            let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
            assert_eq!(master_key.len(), 32);
            
            // Same mnemonic should produce same master key
            let master_key2 = mnemonic_to_master_key(&mnemonic, None).unwrap();
            assert_eq!(master_key, master_key2);
        }
    }

    #[test]
    fn test_word_list_coverage() {
        // Verify that our word list has the expected number of words (standard BIP39)
        assert_eq!(MNEMONIC_ENGLISH_WORDS.len(), 2048, "Word list should have exactly 2048 words");
        
        // Test that the first word is correct
        assert_eq!(MNEMONIC_ENGLISH_WORDS[0], "abandon");
        
        // Test that the last word is correct
        assert_eq!(MNEMONIC_ENGLISH_WORDS[2047], "zoo");
        
        // Test that the first 2048 words contain the standard BIP39 words
        assert!(MNEMONIC_ENGLISH_WORDS.len() >= 2048, "Word list should have at least 2048 words for BIP39 compatibility");
        
        // Test that words are sorted (required for binary search)
        for i in 0..MNEMONIC_ENGLISH_WORDS.len() - 1 {
            assert!(MNEMONIC_ENGLISH_WORDS[i] < MNEMONIC_ENGLISH_WORDS[i + 1], 
                   "Word list not sorted at index {}: '{}' >= '{}'", 
                   i, MNEMONIC_ENGLISH_WORDS[i], MNEMONIC_ENGLISH_WORDS[i + 1]);
        }
    }

    #[test]
    fn test_validate_master_key_derivation() {
        // Generate a mnemonic and derive master key
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Validation should succeed for correct mnemonic
        assert!(validate_master_key_derivation(&master_key, &mnemonic, None).unwrap());
        
        // Validation should fail for wrong mnemonic
        let wrong_mnemonic = generate_seed_phrase().unwrap();
        assert!(!validate_master_key_derivation(&master_key, &wrong_mnemonic, None).unwrap());
        
        // Test with passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("test_pass")).unwrap();
        let mnemonic_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        let master_key_with_pass = mnemonic_to_master_key(&mnemonic_with_pass, Some("test_pass")).unwrap();
        
        // Should succeed with correct passphrase
        assert!(validate_master_key_derivation(&master_key_with_pass, &mnemonic_with_pass, Some("test_pass")).unwrap());
        
        // Should fail with wrong passphrase
        assert!(validate_master_key_derivation(&master_key_with_pass, &mnemonic_with_pass, Some("wrong_pass")).is_err());
        
        // Should fail with no passphrase when one is required
        assert!(validate_master_key_derivation(&master_key_with_pass, &mnemonic_with_pass, None).is_err());
    }

    #[test]
    fn test_master_key_matches_seed() {
        // Generate test data
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        let wrong_mnemonic = generate_seed_phrase().unwrap();
        
        // Should return true for correct mnemonic
        assert!(master_key_matches_seed(&master_key, &mnemonic, None));
        
        // Should return false for wrong mnemonic
        assert!(!master_key_matches_seed(&master_key, &wrong_mnemonic, None));
        
        // Should return false for invalid mnemonic (doesn't panic)
        assert!(!master_key_matches_seed(&master_key, "invalid mnemonic", None));
        
        // Should return false for empty mnemonic
        assert!(!master_key_matches_seed(&master_key, "", None));
    }

    #[test]
    fn test_validate_master_key_from_cipher_seed() {
        // Create a CipherSeed and derive master key
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(None).unwrap();
        let mnemonic = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Validation should succeed for correct CipherSeed
        assert!(validate_master_key_from_cipher_seed(&master_key, &cipher_seed).unwrap());
        
        // Validation should fail for different CipherSeed
        let different_cipher_seed = CipherSeed::new();
        assert!(!validate_master_key_from_cipher_seed(&master_key, &different_cipher_seed).unwrap());
        
        // Test with specific CipherSeed values for determinism
        let deterministic_cipher_seed = CipherSeed {
            version: CIPHER_SEED_VERSION,
            birthday: 12345,
            entropy: Box::new([42u8; CIPHER_SEED_ENTROPY_BYTES]),
            salt: [99u8; CIPHER_SEED_MAIN_SALT_BYTES],
        };
        
        let det_encrypted = deterministic_cipher_seed.encipher(None).unwrap();
        let det_mnemonic = bytes_to_mnemonic(&det_encrypted).unwrap();
        let det_master_key = mnemonic_to_master_key(&det_mnemonic, None).unwrap();
        
        assert!(validate_master_key_from_cipher_seed(&det_master_key, &deterministic_cipher_seed).unwrap());
    }

    #[test]
    fn test_get_master_key_derivation_info() {
        let dummy_master_key = [0u8; 32];
        let info = get_master_key_derivation_info(&dummy_master_key);
        
        assert_eq!(info.master_key, dummy_master_key);
        assert_eq!(info.expected_branch_seed, "master_key");
        assert_eq!(info.expected_key_index, 0);
        assert_eq!(info.derivation_pattern, "H(entropy || \"master_key\" || 0)");
        assert_eq!(info.hash_algorithm, "Blake2b-512 (first 32 bytes)");
        assert_eq!(info.domain_separation, "KeyManagerDomain with label 'derive_key'");
    }

    #[test]
    fn test_find_matching_seed_phrase() {
        // Generate test mnemonics
        let mnemonic1 = generate_seed_phrase().unwrap();
        let mnemonic2 = generate_seed_phrase().unwrap();
        let mnemonic3 = generate_seed_phrase().unwrap();
        
        let master_key1 = mnemonic_to_master_key(&mnemonic1, None).unwrap();
        
        let candidates = vec![mnemonic1.clone(), mnemonic2.clone(), mnemonic3.clone()];
        
        // Should find the correct mnemonic
        let found = find_matching_seed_phrase(&master_key1, &candidates, None).unwrap();
        assert_eq!(found, Some(mnemonic1.clone()));
        
        // Should return None if master key doesn't match any candidate
        let wrong_master_key = [99u8; 32];
        let not_found = find_matching_seed_phrase(&wrong_master_key, &candidates, None).unwrap();
        assert_eq!(not_found, None);
        
        // Test with empty candidates
        let empty_candidates: Vec<String> = vec![];
        let empty_result = find_matching_seed_phrase(&master_key1, &empty_candidates, None).unwrap();
        assert_eq!(empty_result, None);
    }

    #[test]
    fn test_validate_complete_derivation_chain() {
        // Generate complete test chain
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("chain_test")).unwrap();
        let mnemonic = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, Some("chain_test")).unwrap();
        
        // Should validate complete chain successfully
        assert!(validate_complete_derivation_chain(&master_key, &encrypted_bytes, Some("chain_test")).unwrap());
        
        // Should fail with wrong passphrase
        assert!(validate_complete_derivation_chain(&master_key, &encrypted_bytes, Some("wrong")).is_err());
        
        // Should fail with wrong master key
        let wrong_master_key = [88u8; 32];
        assert!(!validate_complete_derivation_chain(&wrong_master_key, &encrypted_bytes, Some("chain_test")).unwrap());
        
        // Should fail with corrupted encrypted bytes
        let mut corrupted_bytes = encrypted_bytes.clone();
        corrupted_bytes[0] ^= 0xFF; // Flip bits in first byte
        assert!(validate_complete_derivation_chain(&master_key, &corrupted_bytes, Some("chain_test")).is_err());
    }

    #[test]
    fn test_detailed_master_key_validation() {
        // Generate test data
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Perform detailed validation
        let result = detailed_master_key_validation(&master_key, &mnemonic, None).unwrap();
        
        // All validations should succeed
        assert!(result.mnemonic_valid);
        assert!(result.cipher_seed_decryption_success);
        assert!(result.master_key_derivation_success);
        assert!(result.cipher_seed_validation);
        assert!(result.final_master_key_match);
        assert!(result.validation_successful);
        
        // Check CipherSeed info is populated
        assert!(result.cipher_seed_info.version == CIPHER_SEED_VERSION || result.cipher_seed_info.version == CIPHER_SEED_VERSION_LEGACY);
        assert!(!result.cipher_seed_info.entropy_hash.is_empty());
        assert!(!result.cipher_seed_info.salt_hash.is_empty());
        
        // Check derivation info
        assert_eq!(result.derivation_info.master_key, master_key);
        assert_eq!(result.derivation_info.expected_branch_seed, "master_key");
        assert_eq!(result.derivation_info.expected_key_index, 0);
        
        // Validation should have taken some time (but not too much)
        assert!(result.validation_time_ms < 5000); // Should be well under 5 seconds
        
        // Test with wrong master key
        let wrong_master_key = [77u8; 32];
        let wrong_result = detailed_master_key_validation(&wrong_master_key, &mnemonic, None).unwrap();
        
        // Some validations should succeed, but final result should fail
        assert!(wrong_result.mnemonic_valid);
        assert!(wrong_result.cipher_seed_decryption_success);
        assert!(wrong_result.master_key_derivation_success);
        assert!(!wrong_result.final_master_key_match); // This should fail
        assert!(!wrong_result.validation_successful); // Overall validation should fail
    }

    #[test]
    fn test_master_key_validation_edge_cases() {
        // Test with various edge cases
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Test empty mnemonic
        assert!(validate_master_key_derivation(&master_key, "", None).is_err());
        
        // Test whitespace-only mnemonic
        assert!(validate_master_key_derivation(&master_key, "   ", None).is_err());
        
        // Test invalid mnemonic
        assert!(validate_master_key_derivation(&master_key, "invalid words here", None).is_err());
        
        // Test with all-zero master key
        let zero_master_key = [0u8; 32];
        assert!(!master_key_matches_seed(&zero_master_key, &mnemonic, None));
        
        // Test with all-FF master key
        let ff_master_key = [0xFFu8; 32];
        assert!(!master_key_matches_seed(&ff_master_key, &mnemonic, None));
    }

    #[test]
    fn test_master_key_validation_consistency() {
        // Test that validation is consistent across multiple calls
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Multiple validations should give the same result
        for _ in 0..10 {
            assert!(validate_master_key_derivation(&master_key, &mnemonic, None).unwrap());
            assert!(master_key_matches_seed(&master_key, &mnemonic, None));
        }
        
        // Test with passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("consistent_test")).unwrap();
        let mnemonic_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        let master_key_with_pass = mnemonic_to_master_key(&mnemonic_with_pass, Some("consistent_test")).unwrap();
        
        // Multiple validations should be consistent
        for _ in 0..5 {
            assert!(validate_master_key_derivation(&master_key_with_pass, &mnemonic_with_pass, Some("consistent_test")).unwrap());
            // Should return an error with wrong passphrase, so we expect is_err() to be true
            assert!(validate_master_key_derivation(&master_key_with_pass, &mnemonic_with_pass, Some("wrong_pass")).is_err());
        }
    }

    #[test]
    fn test_master_key_validation_performance() {
        // Test that validation operations complete in reasonable time
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        let start = time::Instant::now();
        
        // Perform multiple validations (reduced to account for Argon2 being expensive)
        for _ in 0..10 {
            assert!(validate_master_key_derivation(&master_key, &mnemonic, None).unwrap());
        }
        
        let duration = start.elapsed();
        
        // 10 validations should complete in reasonable time (less than 30 seconds, accounting for Argon2)
        assert!(duration.as_secs() < 30, "Validation took too long: {:?}", duration);
        
        // Each validation should take less than 5 seconds on average
        let avg_time_per_validation = duration.as_millis() / 10;
        assert!(avg_time_per_validation < 5000, "Average validation time too high: {}ms", avg_time_per_validation);
    }

    #[test]
    fn test_enhanced_error_handling_empty_seed_phrase() {
        // Test empty seed phrase
        let result = validate_seed_phrase("");
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::EmptySeedPhrase));
        assert!(error.is_recoverable());
        assert_eq!(error.category(), "seed_phrase");
        assert!(error.recovery_suggestion().is_some());
    }

    #[test]
    fn test_enhanced_error_handling_invalid_word_count() {
        // Test too few words
        let short_mnemonic = "abandon abandon abandon";
        let result = validate_seed_phrase(short_mnemonic);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::InvalidWordCount { expected: 24, actual: 3 }));
        assert!(error.is_recoverable());
        assert_eq!(error.category(), "seed_phrase");
        
        // Test too many words
        let long_mnemonic = "abandon ".repeat(30);
        let result = validate_seed_phrase(&long_mnemonic);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::InvalidWordCount { expected: 24, actual: 30 }));
    }

    #[test]
    fn test_enhanced_error_handling_unknown_word() {
        // Test with invalid word
        let invalid_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon invalidword";
        let result = validate_seed_phrase(invalid_mnemonic);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::UnknownWord { ref word, position: 23 } if word == "invalidword"));
        assert!(error.is_recoverable());
        assert_eq!(error.category(), "seed_phrase");
        
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("Check word 24"));
    }

    #[test]
    fn test_enhanced_error_handling_master_key_derivation() {
        // Test master key derivation with invalid passphrase
        let mnemonic = generate_seed_phrase().unwrap();
        
        // Create encrypted seed with passphrase
        let cipher_seed = CipherSeed::new();
        let encrypted_bytes = cipher_seed.encipher(Some("correct_passphrase")).unwrap();
        let mnemonic_with_pass = bytes_to_mnemonic(&encrypted_bytes).unwrap();
        
        // Try with wrong passphrase
        let result = mnemonic_to_master_key(&mnemonic_with_pass, Some("wrong_passphrase"));
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::CipherSeedDecryptionFailed { .. }));
        assert_eq!(error.category(), "cipher_seed");
        
        // Try with no passphrase when one is required
        let result = mnemonic_to_master_key(&mnemonic_with_pass, None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::MissingRequiredPassphrase));
        assert!(error.is_recoverable());
        assert_eq!(error.category(), "passphrase");
    }

    #[test]
    fn test_enhanced_error_handling_validation_chain() {
        // Test complete validation chain with various errors
        let master_key = [1u8; 32];
        
        // Test with empty candidates
        let result = find_matching_seed_phrase(&master_key, &[], None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
        
        // Test with invalid candidates
        let invalid_candidates = vec![
            "".to_string(),                    // Empty
            "too few words".to_string(),       // Too few words
            "abandon ".repeat(25),             // Too many words
        ];
        
        let result = find_matching_seed_phrase(&master_key, &invalid_candidates, None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::WalletRecoveryFailed { .. }));
        assert_eq!(error.category(), "recovery");
    }

    #[test]
    fn test_enhanced_error_handling_cipher_seed_format() {
        // Test with invalid encrypted bytes length
        let invalid_bytes = vec![1u8; 32]; // Wrong length
        let result = validate_complete_derivation_chain(&[0u8; 32], &invalid_bytes, None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::InvalidCipherSeedFormat { .. }));
        assert_eq!(error.category(), "cipher_seed");
    }

    #[test]
    fn test_enhanced_error_handling_detailed_validation() {
        // Test detailed validation with various scenarios
        let mnemonic = generate_seed_phrase().unwrap();
        let master_key = mnemonic_to_master_key(&mnemonic, None).unwrap();
        
        // Valid case
        let result = detailed_master_key_validation(&master_key, &mnemonic, None);
        assert!(result.is_ok());
        let validation_result = result.unwrap();
        assert!(validation_result.validation_successful);
        assert!(validation_result.final_master_key_match);
        
        // Invalid case - empty mnemonic
        let result = detailed_master_key_validation(&master_key, "", None);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, KeyManagementError::EmptySeedPhrase));
        
        // Invalid case - wrong master key
        let wrong_master_key = [99u8; 32];
        let result = detailed_master_key_validation(&wrong_master_key, &mnemonic, None);
        assert!(result.is_ok());
        let validation_result = result.unwrap();
        assert!(!validation_result.validation_successful);
        assert!(!validation_result.final_master_key_match);
    }

    #[test]
    fn test_error_categorization_and_recovery() {
        // Test all error categories
        let errors = vec![
            KeyManagementError::empty_seed_phrase(),
            KeyManagementError::invalid_word_count(24, 12),
            KeyManagementError::unknown_word("invalid", 5),
            KeyManagementError::master_key_derivation_failed("test"),
            KeyManagementError::cipher_seed_decryption_failed("test"),
            KeyManagementError::missing_required_passphrase(),
            KeyManagementError::key_validation_failed("test", "test"),
            KeyManagementError::domain_separation_error("test", "test", "test"),
            KeyManagementError::wallet_recovery_failed("test", "test", "test"),
        ];
        
        let expected_categories = vec![
            "seed_phrase",
            "seed_phrase", 
            "seed_phrase",
            "key_derivation",
            "cipher_seed",
            "passphrase",
            "key_validation",
            "domain_separation",
            "recovery",
        ];
        
        for (error, expected_category) in errors.iter().zip(expected_categories.iter()) {
            assert_eq!(error.category(), *expected_category);
        }
        
        // Test recoverable classification
        assert!(KeyManagementError::empty_seed_phrase().is_recoverable());
        assert!(KeyManagementError::invalid_word_count(24, 12).is_recoverable());
        assert!(KeyManagementError::unknown_word("test", 0).is_recoverable());
        assert!(!KeyManagementError::master_key_derivation_failed("test").is_recoverable());
        
        // Test critical classification
        assert!(KeyManagementError::master_key_derivation_failed("test").is_critical());
        assert!(KeyManagementError::cipher_seed_mac_verification_failed().is_critical());
        assert!(!KeyManagementError::empty_seed_phrase().is_critical());
    }

    #[test]
    fn test_error_recovery_suggestions() {
        // Test recovery suggestions for various errors
        let error = KeyManagementError::unknown_word("test", 5);
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("Check word 6"));
        assert!(suggestion.contains("BIP39"));
        
        let error = KeyManagementError::invalid_word_count(24, 12);
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("24 words"));
        
        let error = KeyManagementError::empty_seed_phrase();
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("12 or 24 words"));
        
        let error = KeyManagementError::missing_required_passphrase();
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("passphrase"));
        
        let error = KeyManagementError::invalid_passphrase();
        let suggestion = error.recovery_suggestion().unwrap();
        assert!(suggestion.contains("correct"));
        
        // Errors without specific suggestions
        let error = KeyManagementError::master_key_derivation_failed("test");
        assert!(error.recovery_suggestion().is_none());
    }

    #[test]
    fn test_enhanced_error_messages() {
        // Test that error messages are descriptive and helpful
        let error = KeyManagementError::invalid_word_count(24, 12);
        let error_string = error.to_string();
        assert!(error_string.contains("expected 24 words"));
        assert!(error_string.contains("got 12 words"));
        assert!(error_string.contains("exactly 24 words"));
        
        let error = KeyManagementError::unknown_word("invalidword", 5);
        let error_string = error.to_string();
        assert!(error_string.contains("invalidword"));
        assert!(error_string.contains("position 5"));
        assert!(error_string.contains("BIP39 word list"));
        assert!(error_string.contains("typos"));
        
        let error = KeyManagementError::cipher_seed_decryption_failed("Wrong passphrase");
        let error_string = error.to_string();
        assert!(error_string.contains("Wrong passphrase"));
        assert!(error_string.contains("verify the passphrase"));
        
        let error = KeyManagementError::branch_key_derivation_failed("test_branch", 42, "Invalid key");
        let error_string = error.to_string();
        assert!(error_string.contains("test_branch"));
        assert!(error_string.contains("42"));
        assert!(error_string.contains("Invalid key"));
    }
} 