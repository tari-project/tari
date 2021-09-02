const PublicKey = require("./publicKey");
const InterfaceFFI = require("./ffiInterface");

class Contact {
  #tari_contact_ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#tari_contact_ptr) {
      this.destroy();
      this.#tari_contact_ptr = ptr;
    } else {
      this.#tari_contact_ptr = ptr;
    }
  }

  getPtr() {
    return this.#tari_contact_ptr;
  }

  getAlias() {
    const alias = InterfaceFFI.contactGetAlias(this.#tari_contact_ptr);
    const result = alias.readCString();
    InterfaceFFI.stringDestroy(alias);
    return result;
  }

  getPubkey() {
    let result = new PublicKey();
    result.pointerAssign(
      InterfaceFFI.contactGetPublicKey(this.#tari_contact_ptr)
    );
    return result;
  }

  getPubkeyHex() {
    let result = "";
    let pk = new PublicKey();
    pk.pointerAssign(InterfaceFFI.contactGetPublicKey(this.#tari_contact_ptr));
    result = pk.getHex();
    pk.destroy();
    return result;
  }

  destroy() {
    if (this.#tari_contact_ptr) {
      InterfaceFFI.contactDestroy(this.#tari_contact_ptr);
      this.#tari_contact_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Contact;
