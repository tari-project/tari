const WalletFFI = require("./walletFFI");

class SeedWords {
  #tari_seed_words_ptr;

  constructor(tari_seed_words_ptr) {
    this.#tari_seed_words_ptr = tari_seed_words_ptr;
  }

  static async fromString(seed_words_text) {
    const seed_words = await WalletFFI.seedWordsCreate();
    const seed_words_list = seed_words_text.split(" ");
    for (const seed_word of seed_words_list) {
      await WalletFFI.seedWordsPushWord(seed_words, seed_word);
    }
    return new SeedWords(seed_words);
  }

  static async fromWallet(wallet) {
    return new SeedWords(await WalletFFI.walletGetSeedWords(wallet));
  }

  getLength() {
    return WalletFFI.seedWordsGetLength(this.#tari_seed_words_ptr);
  }

  getPtr() {
    return this.#tari_seed_words_ptr;
  }

  async getAt(position) {
    const seed_word = await WalletFFI.seedWordsGetAt(
      this.#tari_seed_words_ptr,
      position
    );
    const result = seed_word.readCString();
    await WalletFFI.stringDestroy(seed_word);
    return result;
  }

  destroy() {
    return WalletFFI.seedWordsDestroy(this.#tari_seed_words_ptr);
  }
}

module.exports = SeedWords;
