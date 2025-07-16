use wasm_bindgen::prelude::*;

use crate::tari_address::TariAddress;

/// Derive a public key from a master key, returning it as a hex string.
#[wasm_bindgen]
pub fn make_tari_address() -> Result<String, JsValue> {
    let emoji_string = "🐢🐋🏦💤🐣👣📱🚜🍍🍉🎺🥊📖🔦😷👾🐺🐬👗🔱🌻💍🎢🎪🛵🐋🐊👞🥝🐍🌸📷🔧🎭🐮⏰🍇💯🐛🌴💨🔌🍪📟🎲🐝🤢🎉🔑🌵🚒🐙😍🐝🍑🐜👂🧩⏰🎀🚀🍵👑💐🎮🎮🎣🎒🍬🍳🍸🍷🍶🍯🍵🥄🍭🥐💣";

    let tari_address = TariAddress::from_emoji_string(emoji_string).unwrap();
    let emoji_id = tari_address.to_string();
    Ok(emoji_id)
}
