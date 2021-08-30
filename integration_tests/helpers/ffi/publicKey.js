const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");
const utf8 = require("utf8");

class PublicKey {
  #tari_public_key_ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#tari_public_key_ptr) {
      this.destroy();
      this.#tari_public_key_ptr = ptr;
    } else {
      this.#tari_public_key_ptr = ptr;
    }
  }

  fromPrivateKey(key) {
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyFromPrivateKey(key.getPtr()));
    return result;
  }

  static fromHexString(hex) {
    let sanitize = utf8.encode(hex); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyFromHex(sanitize));
    return result;
  }

  fromEmojiID(emoji) {
    let sanitize = utf8.encode(emoji); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.emojiIdToPublicKey(sanitize));
    return result;
  }

  fromByteVector(byte_vector) {
    let result = new PublicKey();
    result.pointerAssign(InterfaceFFI.publicKeyCreate(byte_vector.getPtr()));
    return result;
  }

  getPtr() {
    return this.#tari_public_key_ptr;
  }

  getBytes() {
    let result = new ByteVector();
    result.pointerAssign(
      InterfaceFFI.publicKeyGetBytes(this.#tari_public_key_ptr)
    );
    return result;
  }

  getHex() {
    const bytes = this.getBytes();
    const length = bytes.getLength();
    let byte_array = new Uint8Array(length);
    for (let i = 0; i < length; i++) {
      byte_array[i] = bytes.getAt(i);
    }
    bytes.destroy();
    let buffer = Buffer.from(byte_array, 0);
    return buffer.toString("hex");
  }

  getEmojiId() {
    const emoji_id = InterfaceFFI.publicKeyToEmojiId(this.#tari_public_key_ptr);
    const result = emoji_id.readCString();
    InterfaceFFI.stringDestroy(emoji_id);
    return result;
  }

  destroy() {
    if (this.#tari_public_key_ptr) {
      InterfaceFFI.publicKeyDestroy(this.#tari_public_key_ptr);
      this.#tari_public_key_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PublicKey;
