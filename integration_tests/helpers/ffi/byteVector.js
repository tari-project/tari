const InterfaceFFI = require("./ffiInterface");

class ByteVector {
  #byte_vector_ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#byte_vector_ptr) {
      this.destroy();
      this.#byte_vector_ptr = ptr;
    } else {
      this.#byte_vector_ptr = ptr;
    }
  }

  fromBytes(input) {
    let buf = Buffer.from(input, "utf-8"); // ensure encoding is utf=8, js default is utf-16
    let len = buf.length; // get the length
    let result = new ByteVector();
    result.pointerAssign(InterfaceFFI.byteVectorCreate(buf, len));
    return result;
  }

  getBytes() {
    let result = [];
    for (let i = 0; i < this.getLength(); i++) {
      result.push(this.getAt(i));
    }
    return result;
  }

  getLength() {
    return InterfaceFFI.byteVectorGetLength(this.#byte_vector_ptr);
  }

  getAt(position) {
    return InterfaceFFI.byteVectorGetAt(this.#byte_vector_ptr, position);
  }

  getPtr() {
    return this.#byte_vector_ptr;
  }

  destroy() {
    if (this.#byte_vector_ptr) {
      InterfaceFFI.byteVectorDestroy(this.#byte_vector_ptr);
      this.#byte_vector_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = ByteVector;
