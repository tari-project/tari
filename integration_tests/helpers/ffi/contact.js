const PublicKey = require("./publicKey");
const WalletFFI = require("./walletFFI");

class Contact {
  #tari_contact_ptr;

  constructor(tari_contact_ptr) {
    this.#tari_contact_ptr = tari_contact_ptr;
  }

  getPtr() {
    return this.#tari_contact_ptr;
  }

  async getAlias() {
    const alias = await WalletFFI.contactGetAlias(this.#tari_contact_ptr);
    const result = alias.readCString();
    await WalletFFI.stringDestroy(alias);
    return result;
  }

  async getPubkey() {
    return new PublicKey(
      await WalletFFI.contactGetPublicKey(this.#tari_contact_ptr)
    );
  }

  destroy() {
    return WalletFFI.contactDestroy(this.#tari_contact_ptr);
  }
}

module.exports = Contact;
