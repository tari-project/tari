const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");

class SeedWords {
  ptr;

  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.ptr) {
      this.destroy();
      this.ptr = ptr;
    } else {
      this.ptr = ptr;
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
    return InterfaceFFI.seedWordsGetLength(this.ptr);
  }

  getPtr() {
    return this.ptr;
  }

  getAt(position) {
    let seed_word = InterfaceFFI.seedWordsGetAt(this.ptr, position);
    let result = seed_word.readCString();
    InterfaceFFI.stringDestroy(seed_word);
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.seedWordsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = SeedWords;
