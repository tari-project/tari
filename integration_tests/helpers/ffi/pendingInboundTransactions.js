const PendingInboundTransaction = require("./pendingInboundTransaction");
const WalletFFI = require("./walletFFI");

class PendingInboundTransactions {
  #tari_pending_inbound_transactions_ptr;

  constructor(tari_pending_inbound_transactions_ptr) {
    this.#tari_pending_inbound_transactions_ptr =
      tari_pending_inbound_transactions_ptr;
  }

  static async fromWallet(wallet) {
    return new PendingInboundTransactions(
      await WalletFFI.walletGetPendingInboundTransactions(wallet)
    );
  }

  getLength() {
    return WalletFFI.pendingInboundTransactionsGetLength(
      this.#tari_pending_inbound_transactions_ptr
    );
  }

  async getAt(position) {
    return new PendingInboundTransaction(
      await WalletFFI.pendingInboundTransactionsGetAt(
        this.#tari_pending_inbound_transactions_ptr,
        position
      )
    );
  }

  destroy() {
    return WalletFFI.pendingInboundTransactionsDestroy(
      this.#tari_pending_inbound_transactions_ptr
    );
  }
}

module.exports = PendingInboundTransactions;
