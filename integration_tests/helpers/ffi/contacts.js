const Contact = require("./contact");
const WalletFFI = require("./walletFFI");

class Contacts {
  #tari_contacts_ptr;

  constructor(tari_contacts_ptr) {
    this.#tari_contacts_ptr = tari_contacts_ptr;
  }

  static async fromWallet(wallet) {
    return new Contacts(await WalletFFI.walletGetContacts(wallet));
  }

  getLength() {
    return WalletFFI.contactsGetLength(this.#tari_contacts_ptr);
  }

  async getAt(position) {
    return new Contact(
      await WalletFFI.contactsGetAt(this.#tari_contacts_ptr, position)
    );
  }

  destroy() {
    return WalletFFI.contactsDestroy(this.#tari_contacts_ptr);
  }
}

module.exports = Contacts;
