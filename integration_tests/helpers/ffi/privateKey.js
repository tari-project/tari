const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");
const utf8 = require("utf8");

class PrivateKey {
  #tari_private_key_ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#tari_private_key_ptr) {
      this.#tari_private_key_ptr = ptr;
      this.destroy();
    } else {
      this.#tari_private_key_ptr = ptr;
    }
  }

  generate() {
    this.#tari_private_key_ptr = InterfaceFFI.privateKeyGenerate();
  }

  fromHexString(hex) {
    let sanitize = utf8.encode(hex); // Make sure it's not UTF-16 encoded (JS default)
    let result = new PrivateKey();
    result.pointerAssign(InterfaceFFI.privateKeyFromHex(sanitize));
    return result;
  }

  fromByteVector(byte_vector) {
    let result = new PrivateKey();
    result.pointerAssign(InterfaceFFI.privateKeyCreate(byte_vector.getPtr()));
    return result;
  }

  getPtr() {
    return this.#tari_private_key_ptr;
  }

  getBytes() {
    let result = new ByteVector();
    result.pointerAssign(
      InterfaceFFI.privateKeyGetBytes(this.#tari_private_key_ptr)
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

  destroy() {
    if (this.#tari_private_key_ptr) {
      InterfaceFFI.privateKeyDestroy(this.#tari_private_key_ptr);
      this.#tari_private_key_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = PrivateKey;
