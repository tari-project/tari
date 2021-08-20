const PendingOutboundTransaction = require("./pendingOutboundTransaction");
const WalletFFI = require("./walletFFI");

class PendingOutboundTransactions {
  #tari_pending_outbound_transactions_ptr;

  constructor(tari_pending_outbound_transactions_ptr) {
    this.#tari_pending_outbound_transactions_ptr =
      tari_pending_outbound_transactions_ptr;
  }

  static async fromWallet(wallet) {
    return new PendingOutboundTransactions(
      await WalletFFI.walletGetPendingOutboundTransactions(wallet)
    );
  }

  getLength() {
    return WalletFFI.pendingOutboundTransactionsGetLength(
      this.#tari_pending_outbound_transactions_ptr
    );
  }

  async getAt(position) {
    return new PendingOutboundTransaction(
      await WalletFFI.pendingOutboundTransactionsGetAt(
        this.#tari_pending_outbound_transactions_ptr,
        position
      )
    );
  }

  destroy() {
    return WalletFFI.pendingOutboundTransactionsDestroy(
      this.#tari_pending_outbound_transactions_ptr
    );
  }
}

module.exports = PendingOutboundTransactions;
