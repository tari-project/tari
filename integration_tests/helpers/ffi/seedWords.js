const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");

class SeedWords {
  #tari_seed_words_ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.#tari_seed_words_ptr) {
      this.destroy();
      this.#tari_seed_words_ptr = ptr;
    } else {
      this.#tari_seed_words_ptr = ptr;
    }
  }

  static fromText(seed_words_text) {
    const seed_words = new SeedWords();
    seed_words.pointerAssign(InterfaceFFI.seedWordsCreate());
    const seed_words_list = seed_words_text.split(" ");
    for (const seed_word of seed_words_list) {
      InterfaceFFI.seedWordsPushWord(
        seed_words.getPtr(),
        utf8.encode(seed_word)
      );
    }
    return seed_words;
  }

  getLength() {
    return InterfaceFFI.seedWordsGetLength(this.#tari_seed_words_ptr);
  }

  getPtr() {
    return this.#tari_seed_words_ptr;
  }

  getAt(position) {
    const seed_word = InterfaceFFI.seedWordsGetAt(
      this.#tari_seed_words_ptr,
      position
    );
    const result = seed_word.readCString();
    InterfaceFFI.stringDestroy(seed_word);
    return result;
  }

  destroy() {
    if (this.#tari_seed_words_ptr) {
      InterfaceFFI.seedWordsDestroy(this.#tari_seed_words_ptr);
      this.#tari_seed_words_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = SeedWords;
