const WalletFFI = require("./walletFFI");

class ByteVector {
  #byte_vector_ptr;

  constructor(byte_vector_ptr) {
    this.#byte_vector_ptr = byte_vector_ptr;
  }

  static async fromBuffer(buffer) {
    let buf = Buffer.from(buffer, "utf-8"); // get the bytes
    let len = buf.length; // get the length
    return new ByteVector(await WalletFFI.byteVectorCreate(buf, len));
  }

  getLength() {
    return WalletFFI.byteVectorGetLength(this.#byte_vector_ptr);
  }

  getAt(position) {
    return WalletFFI.byteVectorGetAt(this.#byte_vector_ptr, position);
  }

  destroy() {
    return WalletFFI.byteVectorDestroy(this.#byte_vector_ptr);
  }
}

module.exports = ByteVector;
