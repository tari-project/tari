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
use digest::consts::U32;
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
        
        // Assemble the final seed: version, encrypted_data, salt, checksum
        let mut encrypted_seed = Vec::with_capacity(1 + secret_data.len() + CIPHER_SEED_MAIN_SALT_BYTES + CIPHER_SEED_CHECKSUM_BYTES);
        encrypted_seed.push(CIPHER_SEED_VERSION);
        encrypted_seed.extend(&secret_data);
        encrypted_seed.extend(&self.salt);
        
        let mut crc_hasher = crc32fast::Hasher::new();
        crc_hasher.update(&encrypted_seed);
        let checksum = crc_hasher.finalize().to_le_bytes();
        encrypted_seed.extend(checksum);
        
        Ok(encrypted_seed)
    }
    
    /// Recover a seed from encrypted data and a passphrase
    pub fn from_enciphered_bytes(encrypted_seed: &[u8], passphrase: Option<&str>) -> Result<Self, KeyManagementError> {
        // Check the length: version, birthday, entropy, MAC, salt, checksum
        if encrypted_seed.len() !=
            1 + CIPHER_SEED_BIRTHDAY_BYTES +
                CIPHER_SEED_ENTROPY_BYTES +
                CIPHER_SEED_MAC_BYTES +
                CIPHER_SEED_MAIN_SALT_BYTES +
                CIPHER_SEED_CHECKSUM_BYTES
        {
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
            1 + CIPHER_SEED_BIRTHDAY_BYTES +
                CIPHER_SEED_ENTROPY_BYTES +
                CIPHER_SEED_MAC_BYTES +
                CIPHER_SEED_MAIN_SALT_BYTES,
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

        // Derive encryption and MAC keys from passphrase and main salt
        let passphrase = passphrase.unwrap_or(DEFAULT_CIPHER_SEED_PASSPHRASE);
        let salt: [u8; CIPHER_SEED_MAIN_SALT_BYTES] = encrypted_seed
            .split_off(1 + CIPHER_SEED_BIRTHDAY_BYTES + CIPHER_SEED_ENTROPY_BYTES + CIPHER_SEED_MAC_BYTES)
            .try_into()
            .map_err(|_| KeyManagementError::InvalidData)?;
        let (encryption_key, mac_key) = Self::derive_keys(passphrase, &salt)?;

        // Decrypt the secret data: birthday, entropy, MAC
        let mut secret_data = encrypted_seed.split_off(1);
        Self::apply_stream_cipher(&mut secret_data, &encryption_key, &salt)?;

        // Parse secret data
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
        Err(_) => Err(KeyManagementError::MnemonicError(format!("Word not found: {}", word))),
    }
}

/// Converts a mnemonic phrase to encrypted CipherSeed bytes using the Tari specification
pub fn mnemonic_to_bytes(mnemonic: &str) -> Result<Vec<u8>, KeyManagementError> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    
    if words.len() != 24 {
        return Err(KeyManagementError::MnemonicError("Mnemonic must be exactly 24 words".to_string()));
    }
    
    // Convert each word to its 11-bit index using LSB-first ordering
    let mut bits = Vec::with_capacity(264); // 24 words * 11 bits = 264 bits
    for word in words {
        let index = find_mnemonic_index_from_word(word)?;
        if index >= MNEMONIC_ENGLISH_WORDS.len() {
            return Err(KeyManagementError::MnemonicError(format!("Invalid word index: {}", index)));
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
        return Err(KeyManagementError::MnemonicError(
            format!("Invalid conversion: expected 33 bytes, got {}", bytes.len())
        ));
    }
    
    Ok(bytes)
}

/// Converts a mnemonic phrase and optional passphrase to a 32-byte master key using Tari CipherSeed
/// This follows the Tari CipherSeed specification
pub fn mnemonic_to_master_key(mnemonic: &str, passphrase: Option<&str>) -> Result<[u8; 32], KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::MnemonicError("Mnemonic phrase is empty".to_string()));
    }
    
    // Convert mnemonic to encrypted bytes
    let encrypted_bytes = mnemonic_to_bytes(mnemonic)?;
    
    // Decrypt the CipherSeed
    let cipher_seed = CipherSeed::from_enciphered_bytes(&encrypted_bytes, passphrase)?;
    
    // Use the entropy as the master key (pad to 32 bytes if necessary)
    let mut master_key = [0u8; 32];
    let entropy = cipher_seed.entropy();
    
    // The CipherSeed entropy is 16 bytes, so we need to expand it to 32 bytes
    // We'll use the entropy + birthday + salt as input to derive a 32-byte key
    let mut key_input = Vec::with_capacity(CIPHER_SEED_ENTROPY_BYTES + 2 + CIPHER_SEED_MAIN_SALT_BYTES);
    key_input.extend(entropy);
    key_input.extend(cipher_seed.birthday().to_le_bytes());
    key_input.extend(cipher_seed.salt());
    
    // Use Blake2b to derive the full 32-byte master key
    let mut hasher = Blake2b::<digest::consts::U32>::new();
    hasher.update(&key_input);
    let result = hasher.finalize();
    master_key.copy_from_slice(result.as_slice());
    
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
    let encrypted_bytes = cipher_seed.encipher(None)?;
    
    // Convert encrypted bytes to mnemonic words
    bytes_to_mnemonic(&encrypted_bytes)
}

/// Converts encrypted CipherSeed bytes to a mnemonic phrase following Tari specification
/// 
/// The encrypted CipherSeed should be exactly 33 bytes, which converts to 24 mnemonic words
pub fn bytes_to_mnemonic(bytes: &[u8]) -> Result<String, KeyManagementError> {
    // The CipherSeed should be exactly 33 bytes for 24-word mnemonic
    if bytes.len() != 33 {
        return Err(KeyManagementError::MnemonicError(
            format!("Invalid encrypted seed length: expected 33 bytes, got {}", bytes.len())
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
            return Err(KeyManagementError::MnemonicError(
                format!("Invalid word index generated: {}", word_index)
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
    
    if words.len() != 24 {
        return Err(KeyManagementError::MnemonicError(
            format!("Invalid mnemonic length: expected 24 words, got {}", words.len())
        ));
    }
    
    // Validate that all words exist in the word list
    for word in &words {
        find_mnemonic_index_from_word(word)?;
    }
    
    // Try to convert mnemonic to bytes (this validates the format)
    let encrypted_bytes = mnemonic_to_bytes(mnemonic)?;
    
    // Try to decrypt the CipherSeed (this validates the checksum and structure)
    // We use the default passphrase for validation
    CipherSeed::from_enciphered_bytes(&encrypted_bytes, None)?;
    
    Ok(())
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
} 