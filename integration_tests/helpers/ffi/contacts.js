const Contact = require("./contact");
const InterfaceFFI = require("./ffiInterface");

class Contacts {
  #tari_contacts_ptr;

  constructor(ptr) {
    this.#tari_contacts_ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.contactsGetLength(this.#tari_contacts_ptr);
  }

  getAt(position) {
    let result = new Contact();
    result.pointerAssign(
      InterfaceFFI.contactsGetAt(this.#tari_contacts_ptr, position)
    );
    return result;
  }

  destroy() {
    if (this.#tari_contacts_ptr) {
      InterfaceFFI.contactsDestroy(this.#tari_contacts_ptr);
      this.#tari_contacts_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Contacts;
