const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");
const CompletedTransaction = require("./completedTransaction");
const CompletedTransactions = require("./completedTransactions");
const PendingInboundTransaction = require("./pendingInboundTransaction");
const PendingInboundTransactions = require("./pendingInboundTransactions");
const PendingOutboundTransactions = require("./pendingOutboundTransactions");
const Contact = require("./contact");
const Contacts = require("./contacts");
const Balance = require("./balance");

const utf8 = require("utf8");

class WalletBalance {
  available = 0;
  timeLocked = 0;
  pendingIn = 0;
  pendingOut = 0;
}

class Wallet {
  ptr;
  balance = new WalletBalance();
  log_path = "";
  receivedTransaction = 0;
  receivedTransactionReply = 0;
  transactionBroadcast = 0;
  transactionMined = 0;
  saf_messages = 0;
  txo_validation_complete = false;
  txo_validation_result = 0;
  tx_validation_complete = false;
  tx_validation_result = 0;
  callback_received_transaction;
  callback_received_transaction_reply;
  callback_received_finalized_transaction;
  callback_transaction_broadcast;
  callback_transaction_mined;
  callback_transaction_mined_unconfirmed;
  callback_direct_send_result;
  callback_store_and_forward_send_result;
  callback_transaction_cancellation;
  callback_balance_updated;
  callback_transaction_validation_complete;
  callback_saf_message_received;
  recoveryProgressCallback;

  getTxoValidationStatus() {
    return {
      txo_validation_complete: this.txo_validation_complete,
      txo_validation_result: this.txo_validation_result,
    };
  }

  getTxValidationStatus() {
    return {
      tx_validation_complete: this.tx_validation_complete,
      tx_validation_result: this.tx_validation_result,
    };
  }

  clearCallbackCounters() {
    this.receivedTransaction =
      this.receivedTransactionReply =
      this.transactionBroadcast =
      this.transactionMined =
      this.saf_messages =
      this.cancelled =
      this.minedunconfirmed =
      this.finalized =
        0;
  }

  getCounters() {
    return {
      received: this.receivedTransaction,
      replyreceived: this.receivedTransactionReply,
      broadcast: this.transactionBroadcast,
      finalized: this.finalized,
      minedunconfirmed: this.minedunconfirmed,
      cancelled: this.cancelled,
      mined: this.transactionMined,
      saf: this.saf_messages,
    };
  }

  constructor(
    comms_config_ptr,
    log_path,
    passphrase,
    seed_words_ptr,
    num_rolling_log_file = 50,
    log_size_bytes = 102400
  ) {
    //region Callbacks
    this.callback_received_transaction =
      InterfaceFFI.createCallbackReceivedTransaction(
        this.onReceivedTransaction
      );
    this.callback_received_transaction_reply =
      InterfaceFFI.createCallbackReceivedTransactionReply(
        this.onReceivedTransactionReply
      );
    this.callback_received_finalized_transaction =
      InterfaceFFI.createCallbackReceivedFinalizedTransaction(
        this.onReceivedFinalizedTransaction
      );
    this.callback_transaction_broadcast =
      InterfaceFFI.createCallbackTransactionBroadcast(
        this.onTransactionBroadcast
      );
    this.callback_transaction_mined =
      InterfaceFFI.createCallbackTransactionMined(this.onTransactionMined);
    this.callback_transaction_mined_unconfirmed =
      InterfaceFFI.createCallbackTransactionMinedUnconfirmed(
        this.onTransactionMinedUnconfirmed
      );
    this.callback_direct_send_result =
      InterfaceFFI.createCallbackDirectSendResult(this.onDirectSendResult);
    this.callback_store_and_forward_send_result =
      InterfaceFFI.createCallbackStoreAndForwardSendResult(
        this.onStoreAndForwardSendResult
      );
    this.callback_transaction_cancellation =
      InterfaceFFI.createCallbackTransactionCancellation(
        this.onTransactionCancellation
      );
    this.callback_txo_validation_complete =
      InterfaceFFI.createCallbackTxoValidationComplete(
        this.onTxoValidationComplete
      );
    this.callback_balance_updated = InterfaceFFI.createCallbackBalanceUpdated(
      this.onBalanceUpdated
    );
    this.callback_transaction_validation_complete =
      InterfaceFFI.createCallbackTransactionValidationComplete(
        this.onTransactionValidationComplete
      );
    this.callback_saf_message_received =
      InterfaceFFI.createCallbackSafMessageReceived(this.onSafMessageReceived);
    this.recoveryProgressCallback = InterfaceFFI.createRecoveryProgressCallback(
      this.onRecoveryProgress
    );
    //endregion

    this.receivedTransaction = 0;
    this.receivedTransactionReply = 0;
    this.transactionBroadcast = 0;
    this.transactionMined = 0;
    this.saf_messages = 0;
    this.cancelled = 0;
    this.minedunconfirmed = 0;
    this.finalized = 0;
    this.recoveryFinished = true;
    let sanitize = null;
    let words = null;
    if (passphrase) {
      sanitize = utf8.encode(passphrase);
    }
    if (seed_words_ptr) {
      words = seed_words_ptr;
    }
    this.log_path = log_path;
    this.ptr = InterfaceFFI.walletCreate(
      comms_config_ptr,
      utf8.encode(this.log_path), //`${this.baseDir}/log/wallet.log`,
      num_rolling_log_file,
      log_size_bytes,
      sanitize,
      words,
      this.callback_received_transaction,
      this.callback_received_transaction_reply,
      this.callback_received_finalized_transaction,
      this.callback_transaction_broadcast,
      this.callback_transaction_mined,
      this.callback_transaction_mined_unconfirmed,
      this.callback_direct_send_result,
      this.callback_store_and_forward_send_result,
      this.callback_transaction_cancellation,
      this.callback_txo_validation_complete,
      this.callback_balance_updated,
      this.callback_transaction_validation_complete,
      this.callback_saf_message_received
    );
  }

  onReceivedTransaction = (ptr) => {
    // refer to outer scope in callback function otherwise this is null
    let tx = new PendingInboundTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received Transaction with txID ${tx.getTransactionID()}`
    );
    tx.destroy();
    this.receivedTransaction += 1;
  };

  onReceivedTransactionReply = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received reply for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.receivedTransactionReply += 1;
  };

  onReceivedFinalizedTransaction = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received finalization for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.finalized += 1;
  };

  onTransactionBroadcast = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was broadcast.`
    );
    tx.destroy();
    this.transactionBroadcast += 1;
  };

  onTransactionMined = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was mined.`
    );
    tx.destroy();
    this.transactionMined += 1;
  };

  onTransactionMinedUnconfirmed = (ptr, confirmations) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} is mined unconfirmed with ${confirmations} confirmations.`
    );
    tx.destroy();
    this.minedunconfirmed += 1;
  };

  onTransactionCancellation = (ptr, reason) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was cancelled with reason code ${reason}.`
    );
    tx.destroy();
    this.cancelled += 1;
  };

  onDirectSendResult = (id, success) => {
    console.log(
      `${new Date().toISOString()} callbackDirectSendResult(${id},${success})`
    );
  };

  onStoreAndForwardSendResult = (id, success) => {
    console.log(
      `${new Date().toISOString()} callbackStoreAndForwardSendResult(${id},${success})`
    );
  };

  onTxoValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackTxoValidationComplete(${request_key},${validation_results})`
    );
    this.txo_validation_complete = true;
    this.txo_validation_result = validation_results;
  };

  onBalanceUpdated = (ptr) => {
    let b = new Balance();
    b.pointerAssign(ptr);
    this.balance.available = b.getAvailable();
    this.balance.timeLocked = b.getTimeLocked();
    this.balance.pendingIn = b.getPendingIncoming();
    this.balance.pendingOut = b.getPendingOutgoing();
    console.log(
      `${new Date().toISOString()} callbackBalanceUpdated: available = ${
        this.balance.available
      },  time locked = ${this.balance.timeLocked}  pending incoming = ${
        this.balance.pendingIn
      } pending outgoing = ${this.balance.pendingOut}`
    );
    b.destroy();
  };

  onTransactionValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackTransactionValidationComplete(${request_key},${validation_results})`
    );
    this.tx_validation_complete = true;
    this.tx_validation_result = validation_results;
  };

  onSafMessageReceived = () => {
    console.log(`${new Date().toISOString()} callbackSafMessageReceived()`);
    this.saf_messages += 1;
  };

  onRecoveryProgress = (a, b, c) => {
    console.log(
      `${new Date().toISOString()} recoveryProgressCallback(${a},${b},${c})`
    );
    if (a === 4) {
      console.log(`Recovery completed, funds recovered: ${c} uT`);
    }
  };

  startRecovery(base_node_pubkey) {
    let node_pubkey = PublicKey.fromHexString(utf8.encode(base_node_pubkey));
    InterfaceFFI.walletStartRecovery(
      this.ptr,
      node_pubkey.getPtr(),
      this.recoveryProgressCallback
    );
    node_pubkey.destroy();
  }

  recoveryInProgress() {
    return InterfaceFFI.walletIsRecoveryInProgress(this.ptr);
  }

  getPublicKey() {
    let ptr = InterfaceFFI.walletGetPublicKey(this.ptr);
    let pk = new PublicKey();
    pk.pointerAssign(ptr);
    let result = pk.getHex();
    pk.destroy();
    return result;
  }

  getBalance() {
    return this.balance;
  }

  pollBalance() {
    let b = new Balance();
    let ptr = InterfaceFFI.walletGetBalance(this.ptr);
    b.pointerAssign(ptr);
    this.balance.available = b.getAvailable();
    this.balance.timeLocked = b.getTimeLocked();
    this.balance.pendingIn = b.getPendingIncoming();
    this.balance.pendingOut = b.getPendingOutgoing();
    b.destroy();
    return this.balance;
  }

  getEmojiId() {
    let ptr = InterfaceFFI.walletGetPublicKey(this.ptr);
    let pk = new PublicKey();
    pk.pointerAssign(ptr);
    let result = pk.getEmojiId();
    pk.destroy();
    return result;
  }

  addBaseNodePeer(public_key_hex, address) {
    let public_key = PublicKey.fromHexString(utf8.encode(public_key_hex));
    let result = InterfaceFFI.walletAddBaseNodePeer(
      this.ptr,
      public_key.getPtr(),
      utf8.encode(address)
    );
    public_key.destroy();
    return result;
  }

  sendTransaction(destination, amount, fee_per_gram, message, one_sided) {
    let dest_public_key = PublicKey.fromHexString(utf8.encode(destination));
    let result = InterfaceFFI.walletSendTransaction(
      this.ptr,
      dest_public_key.getPtr(),
      amount,
      fee_per_gram,
      utf8.encode(message),
      one_sided
    );
    dest_public_key.destroy();
    return result;
  }

  applyEncryption(passphrase) {
    InterfaceFFI.walletApplyEncryption(this.ptr, utf8.encode(passphrase));
  }

  getCompletedTransactions() {
    let list_ptr = InterfaceFFI.walletGetCompletedTransactions(this.ptr);
    return new CompletedTransactions(list_ptr);
  }

  getInboundTransactions() {
    let list_ptr = InterfaceFFI.walletGetPendingInboundTransactions(this.ptr);
    return new PendingInboundTransactions(list_ptr);
  }

  getOutboundTransactions() {
    let list_ptr = InterfaceFFI.walletGetPendingOutboundTransactions(this.ptr);
    return new PendingOutboundTransactions(list_ptr);
  }

  getContacts() {
    let list_ptr = InterfaceFFI.walletGetContacts(this.ptr);
    return new Contacts(list_ptr);
  }

  addContact(alias, pubkey_hex) {
    let public_key = PublicKey.fromHexString(utf8.encode(pubkey_hex));
    let contact = new Contact();
    contact.pointerAssign(
      InterfaceFFI.contactCreate(utf8.encode(alias), public_key.getPtr())
    );
    let result = InterfaceFFI.walletUpsertContact(this.ptr, contact.getPtr());
    contact.destroy();
    public_key.destroy();
    return result;
  }

  removeContact(contact) {
    let result = InterfaceFFI.walletRemoveContact(this.ptr, contact.getPtr());
    contact.destroy();
    return result;
  }

  cancelPendingTransaction(tx_id) {
    return InterfaceFFI.walletCancelPendingTransaction(this.ptr, tx_id);
  }

  startTxoValidation() {
    return InterfaceFFI.walletStartTxoValidation(this.ptr);
  }

  startTxValidation() {
    return InterfaceFFI.walletStartTransactionValidation(this.ptr);
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.walletDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
      this.callback_received_transaction =
        this.callback_received_transaction_reply =
        this.callback_received_finalized_transaction =
        this.callback_transaction_broadcast =
        this.callback_transaction_mined =
        this.callback_transaction_mined_unconfirmed =
        this.callback_direct_send_result =
        this.callback_store_and_forward_send_result =
        this.callback_transaction_cancellation =
        this.callback_txo_validation_complete =
        this.callback_balance_updated =
        this.callback_transaction_validation_complete =
        this.callback_saf_message_received =
        this.recoveryProgressCallback =
          undefined; // clear callback function pointers
    }
  }
}

module.exports = Wallet;
