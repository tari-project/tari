// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const utf8 = require("utf8");
const { expect } = require("chai");

function mnemonicLanguageStepId() {
  return [
    "CHINESE_SIMPLIFIED",
    "ENGLISH",
    "FRENCH",
    "ITALIAN",
    "JAPANESE",
    "KOREAN",
    "SPANISH",
  ];
}

function mnemonicLanguageText() {
  return [
    "ChineseSimplified",
    "English",
    "French",
    "Italian",
    "Japanese",
    "Korean",
    "Spanish",
  ];
}

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

  static getMnemonicWordListForLanguage(language) {
    const index = mnemonicLanguageStepId().indexOf(language);
    if (index < 0) {
      console.log(
        "Mnemonic Language",
        language,
        "not recognized. Select from:\n",
        mnemonicLanguageStepId()
      );
      expect(index < 0).to.equal(false);
    }
    const seed_words = new SeedWords();
    seed_words.pointerAssign(
      InterfaceFFI.seedWordsGetMnemonicWordListForLanguage(
        utf8.encode(mnemonicLanguageText()[index])
      )
    );
    const mnemonicWords = [];
    for (let i = 0; i < seed_words.getLength(); i++) {
      mnemonicWords.push(seed_words.getAt(i));
    }
    seed_words.destroy();
    return mnemonicWords;
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
