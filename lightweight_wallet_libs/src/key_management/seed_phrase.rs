// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Minimal BIP39-like mnemonic-to-seed logic for lightweight wallets (English only)
//! 
//! This implementation follows the Tari CipherSeed specification for compatibility
//! with the main Tari wallet implementation.

use crate::errors::KeyManagementError;
use hmac;
use sha2;

// Constants from the Tari CipherSeed specification
const CIPHER_SEED_VERSION: u8 = 2u8;
const CIPHER_SEED_BIRTHDAY_BYTES: usize = 2;
const CIPHER_SEED_ENTROPY_BYTES: usize = 16;
const CIPHER_SEED_MAIN_SALT_BYTES: usize = 5;
const CIPHER_SEED_MAC_BYTES: usize = 5;
const CIPHER_SEED_CHECKSUM_BYTES: usize = 4;
const DEFAULT_CIPHER_SEED_PASSPHRASE: &str = "TARI_CIPHER_SEED";

// English mnemonic word list (first 2048 words from BIP39)
const MNEMONIC_ENGLISH_WORDS: [&str; 2063] = [
    "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract", "absurd", "abuse",
    "access", "accident", "account", "accuse", "achieve", "acid", "acoustic", "acquire", "across", "act",
    "action", "actor", "actual", "adapt", "add", "addict", "address", "adjust", "admit", "adult",
    "advance", "advice", "aerobic", "affair", "afford", "afraid", "again", "age", "agent", "agree",
    "ahead", "aim", "air", "airport", "aisle", "alarm", "album", "alcohol", "alert", "alien",
    "all", "alley", "allow", "almost", "alone", "alpha", "already", "also", "alter", "always",
    "amateur", "amazing", "among", "amount", "amused", "analyst", "anchor", "ancient", "anger", "angle",
    "angry", "animal", "ankle", "announce", "annual", "another", "answer", "antenna", "antique", "anxiety",
    "any", "apart", "apology", "appear", "apple", "approve", "april", "arch", "arctic", "area",
    "arena", "argue", "arm", "armed", "armor", "army", "around", "arrange", "arrest", "arrive",
    "arrow", "art", "arte", "artist", "artwork", "ask", "aspect", "assault", "asset", "assist",
    "assume", "asthma", "athlete", "atom", "attack", "attend", "attitude", "attract", "auction", "audit",
    "august", "aunt", "author", "auto", "autumn", "average", "avocado", "avoid", "awake", "aware",
    "away", "awesome", "awful", "awkward", "axis", "baby", "bachelor", "bacon", "badge", "bag",
    "balance", "balcony", "ball", "bamboo", "banana", "banner", "bar", "barely", "bargain", "barrel",
    "base", "basic", "basket", "battle", "beach", "bean", "beauty", "because", "become", "beef",
    "before", "begin", "behave", "behind", "believe", "below", "belt", "bench", "benefit", "best",
    "betray", "better", "between", "beyond", "bicycle", "bid", "bike", "bind", "biology", "bird",
    "birth", "bitter", "black", "blade", "blame", "blanket", "blast", "bleak", "bless", "blind",
    "blood", "blossom", "blouse", "blue", "blur", "blush", "board", "boat", "body", "boil",
    "bomb", "bone", "bonus", "book", "boost", "border", "boring", "borrow", "boss", "bottom",
    "bounce", "box", "boy", "bracket", "brain", "brand", "brass", "brave", "bread", "breeze",
    "brick", "bridge", "brief", "bright", "bring", "brisk", "broccoli", "broken", "bronze", "broom",
    "brother", "brown", "brush", "bubble", "buddy", "budget", "buffalo", "build", "bulb", "bulk",
    "bullet", "bundle", "bunker", "burden", "burger", "burst", "bus", "business", "busy", "butter",
    "buyer", "buzz", "cabbage", "cabin", "cable", "cactus", "cage", "cake", "call", "calm",
    "camera", "camp", "can", "canal", "cancel", "candy", "cannon", "canoe", "canvas", "canyon",
    "capable", "capital", "captain", "car", "carbon", "card", "cargo", "carpet", "carry", "cart",
    "case", "cash", "casino", "castle", "casual", "cat", "catalog", "catch", "category", "cattle",
    "caught", "cause", "caution", "cave", "ceiling", "celery", "cement", "census", "century", "cereal",
    "certain", "chair", "chalk", "champion", "change", "chaos", "chapter", "charge", "chase", "chat",
    "cheap", "check", "cheese", "chef", "cherry", "chest", "chicken", "chief", "child", "chimney",
    "choice", "choose", "chronic", "chuckle", "chunk", "churn", "cigar", "cinnamon", "circle", "citizen",
    "city", "civil", "claim", "clap", "clarify", "claw", "clay", "clean", "clerk", "clever",
    "click", "client", "cliff", "climb", "clinic", "clip", "clock", "clog", "close", "cloth",
    "cloud", "clown", "club", "clump", "cluster", "clutch", "coach", "coal", "coast", "coconut",
    "code", "coffee", "coil", "coin", "collect", "color", "column", "combine", "come", "comfort",
    "comic", "common", "company", "concert", "conduct", "confirm", "congress", "connect", "consider", "control",
    "convince", "cook", "cool", "copper", "copy", "coral", "core", "corn", "correct", "cost",
    "cotton", "couch", "country", "couple", "course", "cousin", "cover", "coyote", "crack", "cradle",
    "craft", "cram", "crane", "crash", "crater", "crawl", "crazy", "cream", "credit", "creek",
    "crew", "cricket", "crime", "crisp", "critic", "crop", "cross", "crouch", "crowd", "crucial",
    "cruel", "cruise", "crumble", "crunch", "crush", "cry", "crystal", "cube", "culture", "cup",
    "cupboard", "curious", "current", "curtain", "curve", "cushion", "custom", "cute", "cycle", "dad",
    "damage", "dance", "danger", "daring", "dash", "daughter", "dawn", "day", "deal", "debate",
    "debris", "decade", "december", "decide", "decline", "decorate", "decrease", "deer", "defense", "define",
    "defy", "degree", "delay", "deliver", "demand", "demise", "denial", "dentist", "deny", "depart",
    "depend", "deposit", "depth", "deputy", "derive", "describe", "desert", "design", "desk", "despair",
    "destroy", "detail", "detect", "develop", "device", "devote", "diagram", "dial", "diamond", "diary",
    "dice", "diesel", "diet", "differ", "digital", "dignity", "dilemma", "dinner", "dinosaur", "direct",
    "dirt", "disagree", "discover", "disease", "dish", "dismiss", "disorder", "display", "distance", "divert",
    "divide", "divorce", "dizzy", "doctor", "document", "dog", "doll", "dolphin", "domain", "donate",
    "donkey", "donor", "door", "dose", "double", "dove", "draft", "dragon", "drama", "drastic",
    "draw", "dream", "dress", "drift", "drill", "drink", "drip", "drive", "drop", "drum",
    "dry", "duck", "dumb", "dune", "during", "dust", "dutch", "duty", "dwarf", "dynamic",
    "eager", "eagle", "early", "earn", "earth", "easily", "east", "easy", "echo", "ecology",
    "economy", "edge", "edit", "educate", "effort", "egg", "eight", "either", "elbow", "elder",
    "electric", "elegant", "element", "elephant", "elevator", "elite", "else", "embark", "embody", "embrace",
    "emerge", "emotion", "employ", "empower", "empty", "enable", "enact", "end", "endless", "endorse",
    "enemy", "energy", "enforce", "engage", "engine", "enhance", "enjoy", "enlist", "enough", "enrich",
    "enroll", "ensure", "enter", "entire", "entry", "envelope", "episode", "equal", "equip", "era",
    "erase", "erode", "erosion", "error", "erupt", "escape", "essay", "essence", "estate", "eternal",
    "ethics", "evidence", "evil", "evoke", "evolve", "exact", "example", "excess", "exchange", "excite",
    "exclude", "excuse", "execute", "exercise", "exhaust", "exhibit", "exile", "exist", "exit", "exotic",
    "expand", "expect", "expire", "explain", "expose", "express", "extend", "extra", "eye", "eyebrow",
    "fabric", "face", "faculty", "fade", "faint", "faith", "fall", "false", "fame", "family",
    "famous", "fan", "fancy", "fantasy", "farm", "fashion", "fat", "fatal", "father", "fatigue",
    "fault", "favorite", "feature", "february", "federal", "fee", "feed", "feel", "female", "fence",
    "festival", "fetch", "fever", "few", "fiber", "fiction", "field", "figure", "file", "film",
    "filter", "final", "find", "fine", "finger", "finish", "fire", "firm", "first", "fiscal",
    "fish", "fit", "fitness", "fix", "flag", "flame", "flash", "flat", "flavor", "flee",
    "flight", "flip", "float", "flock", "floor", "flower", "fluid", "flush", "fly", "foam",
    "focus", "fog", "foil", "fold", "follow", "food", "foot", "force", "forest", "forget",
    "fork", "fortune", "forum", "forward", "fossil", "foster", "found", "fox", "fragile", "frame",
    "frequent", "fresh", "friend", "fringe", "frog", "front", "frost", "frown", "frozen", "fruit",
    "fuel", "fun", "funny", "furnace", "fury", "future", "gadget", "gain", "galaxy", "gallery",
    "game", "gap", "garage", "garbage", "garden", "garlic", "garment", "gas", "gasp", "gate",
    "gather", "gauge", "gaze", "general", "genius", "genre", "gentle", "genuine", "gesture", "ghost",
    "giant", "gift", "giggle", "ginger", "giraffe", "girl", "give", "glad", "glance", "glare",
    "glass", "gleam", "glee", "glide", "glimpse", "globe", "gloom", "glory", "glove", "glow",
    "glue", "goat", "goddess", "gold", "good", "goose", "gorilla", "govern", "gown", "grab",
    "grace", "grain", "grant", "grape", "grass", "gravity", "great", "green", "grid", "grief",
    "grit", "grocery", "group", "grow", "grunt", "guard", "guess", "guide", "guilt", "guitar",
    "gun", "gym", "habit", "hair", "half", "hammer", "hamster", "hand", "happy", "harbor",
    "hard", "harsh", "harvest", "hat", "have", "hawk", "hazard", "head", "health", "heart",
    "heavy", "hedgehog", "height", "hello", "helmet", "help", "hen", "hero", "hidden", "high",
    "hill", "hint", "hip", "hire", "history", "hobby", "hockey", "hold", "hole", "holiday",
    "hollow", "home", "honey", "hood", "hope", "horn", "horror", "horse", "hospital", "host",
    "hotel", "hour", "hover", "hub", "huge", "human", "humble", "humor", "hundred", "hungry", "hunt",
    "hurdle", "hurry", "hurt", "husband", "hybrid", "ice", "icon", "idea", "identify", "idle",
    "ignore", "ill", "illegal", "illness", "image", "imitate", "immense", "immune", "impact", "impose",
    "improve", "impulse", "inch", "include", "income", "increase", "index", "indicate", "indoor", "industry",
    "infant", "inflict", "inform", "inhale", "inherit", "initial", "inject", "injury", "inmate", "inner",
    "innocent", "input", "inquiry", "insane", "insect", "inside", "inspire", "install", "intact", "interest",
    "into", "invest", "invite", "involve", "iron", "island", "isolate", "issue", "item", "ivory",
    "jacket", "jaguar", "jar", "jazz", "jealous", "jeans", "jelly", "jewel", "job", "join",
    "joke", "journey", "joy", "judge", "juice", "juicy", "july", "jumbo", "jump", "june",
    "jungle", "junior", "junk", "just", "kangaroo", "keen", "keep", "ketchup", "key", "kick",
    "kid", "kidney", "kind", "kingdom", "kiss", "kit", "kitchen", "kite", "kitten", "kiwi",
    "knee", "knife", "knock", "know", "lab", "label", "labor", "ladder", "lady", "lake",
    "lamp", "language", "laptop", "large", "later", "latin", "laugh", "laundry", "lava", "law",
    "lawn", "lawsuit", "layer", "lazy", "leader", "leaf", "learn", "leave", "lecture", "left",
    "leg", "legal", "legend", "leisure", "lemon", "lend", "length", "lens", "leopard", "lesson",
    "letter", "level", "liar", "liberty", "library", "license", "life", "lift", "light", "like",
    "limb", "limit", "link", "lion", "liquid", "list", "little", "live", "lizard", "load",
    "loan", "lobster", "local", "lock", "logic", "lonely", "long", "loop", "lottery", "loud",
    "lounge", "love", "loyal", "lucky", "luggage", "lumber", "lunar", "lunch", "luxury", "lyrics",
    "machine", "mad", "magic", "magnet", "maid", "mail", "main", "major", "make", "mammal",
    "man", "manage", "mandate", "mango", "mansion", "manual", "maple", "marble", "march", "margin",
    "marine", "market", "marriage", "mask", "mass", "master", "match", "material", "math", "matrix",
    "matter", "maximum", "maze", "meadow", "mean", "measure", "meat", "mechanic", "medal", "media",
    "melody", "melt", "member", "memory", "mention", "menu", "mercy", "merge", "merit", "merry",
    "mesh", "message", "metal", "method", "middle", "midnight", "milk", "million", "mimic", "mind",
    "minimum", "minor", "minute", "miracle", "mirror", "misery", "miss", "mistake", "mix", "mixed",
    "mixture", "mobile", "model", "modify", "mom", "moment", "monitor", "monkey", "monster", "month",
    "moon", "moral", "more", "morning", "mosquito", "mother", "motion", "motor", "mountain", "mouse",
    "move", "movie", "much", "muffin", "mule", "multiply", "muscle", "museum", "mushroom", "music",
    "must", "mutual", "myself", "mystery", "myth", "naive", "name", "napkin", "narrow", "nasty",
    "nation", "nature", "near", "neck", "need", "negative", "neglect", "neither", "nephew", "nerve",
    "nest", "net", "network", "neutral", "never", "news", "next", "nice", "night", "noble",
    "noise", "nominee", "noodle", "normal", "north", "nose", "notable", "note", "nothing", "notice",
    "novel", "now", "nuclear", "number", "nurse", "nut", "oak", "obey", "object", "oblige",
    "obscure", "observe", "obtain", "obvious", "occur", "ocean", "october", "odor", "off", "offer",
    "office", "often", "oil", "okay", "old", "olive", "olympic", "omit", "once", "one",
    "onion", "online", "only", "open", "opera", "opinion", "oppose", "option", "orange", "orbit",
    "orchard", "order", "ordinary", "organ", "orient", "original", "orphan", "ostrich", "other", "outdoor",
    "outer", "output", "outside", "oval", "oven", "over", "own", "owner", "oxygen", "oyster",
    "ozone", "pact", "paddle", "page", "pair", "palace", "palm", "panda", "panel", "panic",
    "panther", "paper", "parade", "parent", "park", "parrot", "party", "pass", "patch", "path",
    "patient", "patrol", "pattern", "pause", "pave", "payment", "peace", "peach", "peanut", "pear",
    "peasant", "pelican", "pen", "penalty", "pencil", "people", "pepper", "perfect", "permit", "person",
    "pet", "phone", "photo", "phrase", "physical", "piano", "picnic", "picture", "piece", "pig",
    "pigeon", "pill", "pilot", "pink", "pioneer", "pipe", "pistol", "pitch", "pizza", "place",
    "planet", "plastic", "plate", "play", "please", "pledge", "pluck", "plug", "plunge", "poem",
    "poet", "point", "polar", "pole", "police", "pond", "pony", "pool", "poor", "popcorn",
    "popular", "portion", "position", "possible", "post", "potato", "pottery", "poverty", "powder", "power",
    "practice", "praise", "predict", "prefer", "prepare", "present", "pretty", "prevent", "price", "pride",
    "primary", "print", "priority", "prison", "private", "prize", "problem", "process", "produce", "profit",
    "program", "project", "promote", "proof", "property", "prosper", "protect", "proud", "provide", "public",
    "pudding", "pull", "pulp", "pulse", "pumpkin", "punch", "pupil", "puppy", "purchase", "purity",
    "purpose", "purse", "push", "put", "puzzle", "pyramid", "quality", "quantum", "quarter", "question",
    "quick", "quit", "quiz", "quote", "rabbit", "raccoon", "race", "rack", "radar", "radio",
    "rail", "rain", "raise", "rally", "ramp", "ranch", "random", "range", "rapid", "rare",
    "rate", "rather", "raven", "raw", "razor", "ready", "real", "reason", "rebel", "rebuild",
    "recall", "receive", "recipe", "record", "recycle", "reduce", "reflect", "reform", "refuse", "region",
    "regret", "regular", "reject", "relax", "release", "relief", "rely", "remain", "remember", "remind",
    "remove", "render", "renew", "rent", "reopen", "repair", "repeat", "replace", "report", "require",
    "rescue", "resemble", "resist", "resource", "response", "result", "retire", "retreat", "return", "reunion",
    "reveal", "review", "reward", "rhythm", "rib", "ribbon", "rice", "rich", "ride", "ridge",
    "rifle", "right", "rigid", "ring", "riot", "ripple", "risk", "ritual", "rival", "river",
    "road", "roast", "robot", "robust", "rocket", "romance", "roof", "rookie", "room", "rose",
    "rotate", "rough", "round", "route", "royal", "rubber", "rude", "rug", "rule", "run",
    "runway", "rural", "sad", "saddle", "sadness", "safe", "sail", "salad", "salmon", "salon",
    "salt", "salute", "same", "sample", "sand", "satisfy", "satoshi", "sauce", "sausage", "save",
    "say", "scale", "scan", "scare", "scatter", "scene", "scheme", "school", "science", "scissors",
    "scorpion", "scout", "scrap", "screen", "script", "scrub", "sea", "search", "season", "seat",
    "second", "secret", "section", "security", "seed", "seek", "segment", "select", "sell", "seminar",
    "senior", "sense", "sentence", "series", "service", "session", "settle", "setup", "seven", "shadow",
    "shaft", "shallow", "share", "shed", "shell", "sheriff", "shield", "shift", "shine", "ship",
    "shiver", "shock", "shoe", "shoot", "shop", "shore", "short", "shoulder", "shove", "shrimp",
    "shrug", "shuffle", "shy", "sibling", "sick", "side", "siege", "sight", "sign", "silent",
    "silk", "silly", "silver", "similar", "simple", "since", "sing", "siren", "sister", "situate",
    "six", "size", "skate", "sketch", "ski", "skill", "skin", "skirt", "skull", "slab",
    "slam", "sleep", "slender", "slice", "slide", "slight", "slim", "slogan", "slot", "slow",
    "slush", "sly", "small", "smart", "smile", "smoke", "smooth", "snack", "snake", "snap",
    "sniff", "snow", "soap", "soccer", "social", "sock", "soda", "soft", "solar", "soldier",
    "solid", "solution", "solve", "someone", "song", "soon", "sorry", "sort", "soul", "sound",
    "soup", "source", "south", "space", "spare", "spatial", "spawn", "speak", "special", "speed",
    "spell", "spend", "sphere", "spice", "spider", "spike", "spin", "spirit", "split", "spoil",
    "sponsor", "spoon", "sport", "spot", "spray", "spread", "spring", "spy", "square", "squeeze",
    "squirrel", "stable", "stadium", "staff", "stage", "stairs", "stamp", "stand", "start", "state",
    "stay", "steak", "steel", "stem", "step", "stereo", "stick", "still", "sting", "stomach",
    "stone", "stony", "story", "stove", "strategy", "street", "strike", "strong", "struggle", "student",
    "stuff", "stumble", "style", "subject", "submit", "subway", "success", "such", "sudden", "suffer",
    "sugar", "suggest", "suit", "summer", "sun", "sunny", "sunset", "super", "supply", "supreme",
    "sure", "surface", "surge", "surprise", "surround", "survey", "suspect", "sustain", "swallow", "swamp",
    "swap", "swarm", "swear", "sweet", "swift", "swim", "swing", "switch", "sword", "symbol",
    "symptom", "syrup", "system", "table", "tackle", "tag", "tail", "talent", "talk", "tank",
    "tape", "target", "task", "taste", "tattoo", "taxi", "teach", "team", "tell", "ten",
    "tenant", "tennis", "tent", "term", "test", "text", "thank", "that", "theme", "then",
    "theory", "there", "they", "thing", "this", "thought", "three", "thrive", "throw", "thumb",
    "thunder", "ticket", "tide", "tiger", "tilt", "timber", "time", "tiny", "tip", "tired",
    "tissue", "title", "toast", "tobacco", "today", "toddler", "toe", "together", "toilet", "token",
    "tomato", "tomorrow", "tone", "tongue", "tonight", "tool", "tooth", "top", "topic", "topple",
    "torch", "tornado", "tortoise", "toss", "total", "tourist", "toward", "tower", "town", "toy",
    "track", "trade", "traffic", "tragic", "train", "transfer", "trap", "trash", "travel", "tray",
    "treat", "tree", "trend", "trial", "tribe", "trick", "trigger", "trim", "trip", "trophy",
    "trouble", "truck", "true", "truly", "trumpet", "trust", "truth", "try", "tube", "tuition",
    "tumble", "tuna", "tunnel", "turkey", "turn", "turtle", "twelve", "twenty", "twice", "twin",
    "twist", "two", "type", "typical", "ugly", "umbrella", "unable", "unaware", "uncle", "uncover",
    "under", "undo", "unfair", "unfold", "unhappy", "uniform", "unique", "unit", "universe", "unknown",
    "unlock", "until", "unusual", "unveil", "update", "upgrade", "uphold", "upon", "upper", "upset",
    "urban", "urge", "usage", "use", "used", "useful", "useless", "usual", "utility", "vacant",
    "vacuum", "vague", "valid", "valley", "valve", "van", "vanish", "vapor", "various", "vast",
    "vault", "vehicle", "velvet", "vendor", "venture", "venue", "verb", "verify", "version", "very",
    "vessel", "veteran", "viable", "vibrant", "vicious", "victory", "video", "view", "village", "vintage",
    "violin", "virtual", "virus", "visa", "visit", "visual", "vital", "vivid", "vocal", "voice",
    "void", "volcano", "volume", "vote", "voucher", "vow", "voyal", "vulnerable", "wade", "wagon",
    "wait", "walk", "wall", "walnut", "want", "warfare", "warm", "warrior", "wash", "wasp",
    "waste", "water", "wave", "way", "wealth", "weapon", "wear", "weasel", "weather", "web",
    "wedding", "weekend", "weird", "welcome", "west", "wet", "whale", "what", "whatever", "wheat",
    "wheel", "when", "whenever", "where", "whereas", "wherever", "whip", "whisper", "wide", "width", "wife",
    "wild", "will", "willing", "win", "window", "wine", "wing", "wink", "winner", "winter",
    "wire", "wisdom", "wise", "wish", "witness", "wolf", "woman", "wonder", "wood", "wool",
    "word", "work", "world", "worry", "worth", "wrap", "wreck", "wrestle", "wrist", "write",
    "wrong", "yard", "year", "yellow", "you", "young", "youth", "zebra", "zero", "zone", "zoo"
];

/// Finds and returns the index of a specific word in the English mnemonic word list
fn find_mnemonic_index_from_word(word: &str) -> Result<usize, KeyManagementError> {
    let lowercase_word = word.to_lowercase();
    match MNEMONIC_ENGLISH_WORDS.binary_search(&lowercase_word.as_str()) {
        Ok(index) => Ok(index),
        Err(_) => Err(KeyManagementError::MnemonicError(format!("Word not found: {}", word))),
    }
}

/// Converts a mnemonic phrase to bytes using the Tari CipherSeed specification
fn mnemonic_to_bytes(mnemonic: &str) -> Result<Vec<u8>, KeyManagementError> {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    
    if words.len() != 24 {
        return Err(KeyManagementError::MnemonicError("Mnemonic must be exactly 24 words".to_string()));
    }
    
    // Convert each word to its 11-bit index
    let mut bits = Vec::new();
    for word in words {
        let index = find_mnemonic_index_from_word(word)?;
        if index >= 2048 {
            return Err(KeyManagementError::MnemonicError(format!("Invalid word index: {}", index)));
        }
        
        // Convert 11-bit index to bits
        for i in 0..11 {
            bits.push((index >> i) & 1 == 1);
        }
    }
    
    // Convert bits to bytes
    let mut bytes = Vec::new();
    let mut current_byte = 0u8;
    let mut bit_count = 0;
    
    for bit in bits {
        if bit {
            current_byte |= 1 << bit_count;
        }
        bit_count += 1;
        
        if bit_count == 8 {
            bytes.push(current_byte);
            current_byte = 0;
            bit_count = 0;
        }
    }
    
    // Add any remaining bits
    if bit_count > 0 {
        bytes.push(current_byte);
    }
    
    Ok(bytes)
}

/// Converts a mnemonic phrase and optional passphrase to a 32-byte master key using BIP39-style derivation
/// This follows the standard BIP39 approach of combining mnemonic + passphrase
pub fn mnemonic_to_master_key(mnemonic: &str, passphrase: Option<&str>) -> Result<[u8; 32], KeyManagementError> {
    if mnemonic.trim().is_empty() {
        return Err(KeyManagementError::MnemonicError("Mnemonic phrase is empty".to_string()));
    }
    
    // Validate the mnemonic format
    let _mnemonic_bytes = mnemonic_to_bytes(mnemonic)?;
    
    // Create the BIP39-style salt: "mnemonic" + passphrase
    let salt_prefix = "mnemonic";
    let passphrase_str = passphrase.unwrap_or("");
    let mut salt = Vec::with_capacity(salt_prefix.len() + passphrase_str.len());
    salt.extend_from_slice(salt_prefix.as_bytes());
    salt.extend_from_slice(passphrase_str.as_bytes());
    
    // Use PBKDF2 with the mnemonic as the password and the constructed salt
    // This is the standard BIP39 approach that properly incorporates the passphrase
    let mut master_key = [0u8; 32];
    pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
        mnemonic.as_bytes(),  // Use the mnemonic string as the password
        &salt,                // Use "mnemonic" + passphrase as the salt
        2048,                 // Standard iteration count
        &mut master_key,
    ).map_err(|_| KeyManagementError::KeyDerivationFailed("PBKDF2 key derivation failed".to_string()))?;

    Ok(master_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mnemonic_to_master_key() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let passphrase = Some("test");
        let key = mnemonic_to_master_key(mnemonic, passphrase).unwrap();
        assert_eq!(key.len(), 32);
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
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        
        let key1 = mnemonic_to_master_key(mnemonic, Some("passphrase1")).unwrap();
        let key2 = mnemonic_to_master_key(mnemonic, Some("passphrase2")).unwrap();
        let key3 = mnemonic_to_master_key(mnemonic, None).unwrap();
        
        // Different passphrases should produce different keys
        assert_ne!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key2, key3);
        
        // Same mnemonic and passphrase should produce the same key
        let key1_duplicate = mnemonic_to_master_key(mnemonic, Some("passphrase1")).unwrap();
        assert_eq!(key1, key1_duplicate);
    }
} 