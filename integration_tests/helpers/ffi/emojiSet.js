const InterfaceFFI = require("./ffiInterface");

class EmojiSet {
  #emoji_set_ptr;

  constructor() {
    this.#emoji_set_ptr = InterfaceFFI.getEmojiSet();
  }

  getLength() {
    return InterfaceFFI.emojiSetGetLength(this.#emoji_set_ptr);
  }

  getAt(position) {
    return InterfaceFFI.emojiSetGetAt(this.#emoji_set_ptr, position);
  }

  list() {
    let set = [];
    for (let i = 0; i < this.getLength(); i++) {
      let item = this.getAt(i);
      set.push(Buffer.from(item.getBytes(), "utf-8").toString());
      item.destroy();
    }
    return set;
  }

  destroy() {
    if (this.#emoji_set_ptr) {
      InterfaceFFI.byteVectorDestroy(this.#emoji_set_ptr);
      this.#emoji_set_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = EmojiSet;
