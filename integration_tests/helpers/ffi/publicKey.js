const WalletFFI = require("./walletFFI");
const ByteVector = require("./byteVector");
const utf8 = require("utf8");

class PublicKey {
  #tari_public_key_ptr;

  constructor(public_key) {
    this.#tari_public_key_ptr = public_key;
  }

  static fromPubkey(public_key) {
    return new PublicKey(public_key);
  }

  static async fromWallet(wallet) {
    return new PublicKey(await WalletFFI.walletGetPublicKey(wallet));
  }

  static async fromString(public_key_hex) {
    let sanitize = utf8.encode(public_key_hex); // Make sure it's not UTF-16 encoded (JS default)
    return new PublicKey(await WalletFFI.publicKeyFromHex(sanitize));
  }

  static async fromBytes(bytes) {
    return new PublicKey(await WalletFFI.publicKeyCreate(bytes));
  }

  getPtr() {
    return this.#tari_public_key_ptr;
  }

  async getBytes() {
    return new ByteVector(
      await WalletFFI.publicKeyGetBytes(this.#tari_public_key_ptr)
    );
  }

  async getHex() {
    const bytes = await this.getBytes();
    const length = await bytes.getLength();
    let byte_array = new Uint8Array(length);
    for (let i = 0; i < length; ++i) {
      byte_array[i] = await bytes.getAt(i);
    }
    await bytes.destroy();
    let buffer = Buffer.from(byte_array, 0);
    return buffer.toString("hex");
  }

  async getEmojiId() {
    const emoji_id = await WalletFFI.publicKeyToEmojiId(
      this.#tari_public_key_ptr
    );
    const result = emoji_id.readCString();
    await WalletFFI.stringDestroy(emoji_id);
    return result;
  }

  destroy() {
    return WalletFFI.publicKeyDestroy(this.#tari_public_key_ptr);
  }
}

module.exports = PublicKey;
