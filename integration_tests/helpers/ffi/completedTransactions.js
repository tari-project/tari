const CompletedTransaction = require("./completedTransaction");
const InterfaceFFI = require("./ffiInterface");

class CompletedTransactions {
  ptr;

  constructor(ptr) {
    this.ptr = ptr;
  }

  getLength() {
    return InterfaceFFI.completedTransactionsGetLength(this.ptr);
  }

  getAt(position) {
    let result = new CompletedTransaction();
    result.pointerAssign(
      InterfaceFFI.completedTransactionsGetAt(this.ptr, position)
    );
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.completedTransactionsDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CompletedTransactions;
