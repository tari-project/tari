const CompletedTransaction = require("./completedTransaction");
const InterfaceFFI = require("./ffiInterface");

class CompletedTransactions {
  #tari_completed_transactions_ptr;

  constructor(ptr) {
    this.#tari_completed_transactions_ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.completedTransactionsGetLength(
      this.#tari_completed_transactions_ptr
    );
  }

  getAt(position) {
    let result = new CompletedTransaction();
    result.pointerAssign(
      InterfaceFFI.completedTransactionsGetAt(
        this.#tari_completed_transactions_ptr,
        position
      )
    );
    return result;
  }

  destroy() {
    if (this.#tari_completed_transactions_ptr) {
      InterfaceFFI.completedTransactionsDestroy(
        this.#tari_completed_transactions_ptr
      );
      this.#tari_completed_transactions_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CompletedTransactions;
