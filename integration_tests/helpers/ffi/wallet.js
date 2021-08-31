const InterfaceFFI = require("./ffiInterface");
const PublicKey = require("./publicKey");
const CompletedTransaction = require("./completedTransaction");
const CompletedTransactions = require("./completedTransactions");
const PendingInboundTransaction = require("./pendingInboundTransaction");
const PendingInboundTransactions = require("./pendingInboundTransactions");
const PendingOutboundTransactions = require("./pendingOutboundTransactions");
const Contact = require("./contact");
const Contacts = require("./contacts");

const utf8 = require("utf8");

class Wallet {
  #wallet_ptr;
  #log_path = "";
  receivedTransaction = 0;
  receivedTransactionReply = 0;
  transactionBroadcast = 0;
  transactionMined = 0;
  saf_messages = 0;

  utxo_validation_complete = false;
  utxo_validation_result = 0;
  stxo_validation_complete = false;
  stxo_validation_result = 0;

  getUtxoValidationStatus() {
    return {
      utxo_validation_complete: this.utxo_validation_complete,
      utxo_validation_result: this.utxo_validation_result,
    };
  }

  getStxoValidationStatus() {
    return {
      stxo_validation_complete: this.stxo_validation_complete,
      stxo_validation_result: this.stxo_validation_result,
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
    this.#log_path = log_path;
    this.#wallet_ptr = InterfaceFFI.walletCreate(
      comms_config_ptr,
      utf8.encode(this.#log_path), //`${this.baseDir}/log/wallet.log`,
      num_rolling_log_file,
      log_size_bytes,
      sanitize,
      words,
      this.#callback_received_transaction,
      this.#callback_received_transaction_reply,
      this.#callback_received_finalized_transaction,
      this.#callback_transaction_broadcast,
      this.#callback_transaction_mined,
      this.#callback_transaction_mined_unconfirmed,
      this.#callback_direct_send_result,
      this.#callback_store_and_forward_send_result,
      this.#callback_transaction_cancellation,
      this.#callback_utxo_validation_complete,
      this.#callback_stxo_validation_complete,
      this.#callback_invalid_txo_validation_complete,
      this.#callback_transaction_validation_complete,
      this.#callback_saf_message_received
    );
  }

  //region Callbacks
  #onReceivedTransaction = (ptr) => {
    // refer to outer scope in callback function otherwise this is null
    let tx = new PendingInboundTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received Transaction with txID ${tx.getTransactionID()}`
    );
    tx.destroy();
    this.receivedTransaction += 1;
  };

  #onReceivedTransactionReply = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received reply for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.receivedTransactionReply += 1;
  };

  #onReceivedFinalizedTransaction = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received finalization for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.finalized += 1;
  };

  #onTransactionBroadcast = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was broadcast.`
    );
    tx.destroy();
    this.transactionBroadcast += 1;
  };

  #onTransactionMined = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was mined.`
    );
    tx.destroy();
    this.transactionMined += 1;
  };

  #onTransactionMinedUnconfirmed = (ptr, confirmations) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} is mined unconfirmed with ${confirmations} confirmations.`
    );
    tx.destroy();
    this.minedunconfirmed += 1;
  };

  #onTransactionCancellation = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was cancelled`
    );
    tx.destroy();
    this.cancelled += 1;
  };

  #onDirectSendResult = (id, success) => {
    console.log(
      `${new Date().toISOString()} callbackDirectSendResult(${id},${success})`
    );
  };

  #onStoreAndForwardSendResult = (id, success) => {
    console.log(
      `${new Date().toISOString()} callbackStoreAndForwardSendResult(${id},${success})`
    );
  };

  #onUtxoValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackUtxoValidationComplete(${request_key},${validation_results})`
    );
    this.utxo_validation_complete = true;
    this.utxo_validation_result = validation_results;
  };

  #onStxoValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackStxoValidationComplete(${request_key},${validation_results})`
    );
    this.stxo_validation_complete = true;
    this.stxo_validation_result = validation_results;
  };

  #onInvalidTxoValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackInvalidTxoValidationComplete(${request_key},${validation_results})`
    );
    //this.invalidtxo_validation_complete = true;
    //this.invalidtxo_validation_result = validation_results;
  };

  #onTransactionValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackTransactionValidationComplete(${request_key},${validation_results})`
    );
    //this.transaction_validation_complete = true;
    //this.transaction_validation_result = validation_results;
  };

  #onSafMessageReceived = () => {
    console.log(`${new Date().toISOString()} callbackSafMessageReceived()`);
    this.saf_messages += 1;
  };

  #onRecoveryProgress = (a, b, c) => {
    console.log(
      `${new Date().toISOString()} recoveryProgressCallback(${a},${b},${c})`
    );
    if (a === 4) {
      console.log(`Recovery completed, funds recovered: ${c} uT`);
    }
  };

  #callback_received_transaction =
    InterfaceFFI.createCallbackReceivedTransaction(this.#onReceivedTransaction);
  #callback_received_transaction_reply =
    InterfaceFFI.createCallbackReceivedTransactionReply(
      this.#onReceivedTransactionReply
    );
  #callback_received_finalized_transaction =
    InterfaceFFI.createCallbackReceivedFinalizedTransaction(
      this.#onReceivedFinalizedTransaction
    );
  #callback_transaction_broadcast =
    InterfaceFFI.createCallbackTransactionBroadcast(
      this.#onTransactionBroadcast
    );
  #callback_transaction_mined = InterfaceFFI.createCallbackTransactionMined(
    this.#onTransactionMined
  );
  #callback_transaction_mined_unconfirmed =
    InterfaceFFI.createCallbackTransactionMinedUnconfirmed(
      this.#onTransactionMinedUnconfirmed
    );
  #callback_direct_send_result = InterfaceFFI.createCallbackDirectSendResult(
    this.#onDirectSendResult
  );
  #callback_store_and_forward_send_result =
    InterfaceFFI.createCallbackStoreAndForwardSendResult(
      this.#onStoreAndForwardSendResult
    );
  #callback_transaction_cancellation =
    InterfaceFFI.createCallbackTransactionCancellation(
      this.#onTransactionCancellation
    );
  #callback_utxo_validation_complete =
    InterfaceFFI.createCallbackUtxoValidationComplete(
      this.#onUtxoValidationComplete
    );
  #callback_stxo_validation_complete =
    InterfaceFFI.createCallbackStxoValidationComplete(
      this.#onStxoValidationComplete
    );
  #callback_invalid_txo_validation_complete =
    InterfaceFFI.createCallbackInvalidTxoValidationComplete(
      this.#onInvalidTxoValidationComplete
    );
  #callback_transaction_validation_complete =
    InterfaceFFI.createCallbackTransactionValidationComplete(
      this.#onTransactionValidationComplete
    );
  #callback_saf_message_received =
    InterfaceFFI.createCallbackSafMessageReceived(this.#onSafMessageReceived);
  #recoveryProgressCallback = InterfaceFFI.createRecoveryProgressCallback(
    this.#onRecoveryProgress
  );
  //endregion

  startRecovery(base_node_pubkey) {
    let node_pubkey = PublicKey.fromHexString(utf8.encode(base_node_pubkey));
    InterfaceFFI.walletStartRecovery(
      this.#wallet_ptr,
      node_pubkey.getPtr(),
      this.#recoveryProgressCallback
    );
    node_pubkey.destroy();
  }

  recoveryInProgress() {
    return InterfaceFFI.walletIsRecoveryInProgress(this.#wallet_ptr);
  }

  getPublicKey() {
    let ptr = InterfaceFFI.walletGetPublicKey(this.#wallet_ptr);
    let pk = new PublicKey();
    pk.pointerAssign(ptr);
    let result = pk.getHex();
    pk.destroy();
    return result;
  }

  getEmojiId() {
    let ptr = InterfaceFFI.walletGetPublicKey(this.#wallet_ptr);
    let pk = new PublicKey();
    pk.pointerAssign(ptr);
    let result = pk.getEmojiId();
    pk.destroy();
    return result;
  }

  getBalance() {
    let available = InterfaceFFI.walletGetAvailableBalance(this.#wallet_ptr);
    let pendingIncoming = InterfaceFFI.walletGetPendingIncomingBalance(
      this.#wallet_ptr
    );
    let pendingOutgoing = InterfaceFFI.walletGetPendingOutgoingBalance(
      this.#wallet_ptr
    );
    return {
      pendingIn: pendingIncoming,
      pendingOut: pendingOutgoing,
      available: available,
    };
  }

  addBaseNodePeer(public_key_hex, address) {
    let public_key = PublicKey.fromHexString(utf8.encode(public_key_hex));
    let result = InterfaceFFI.walletAddBaseNodePeer(
      this.#wallet_ptr,
      public_key.getPtr(),
      utf8.encode(address)
    );
    public_key.destroy();
    return result;
  }

  sendTransaction(destination, amount, fee_per_gram, message) {
    let dest_public_key = PublicKey.fromHexString(utf8.encode(destination));
    let result = InterfaceFFI.walletSendTransaction(
      this.#wallet_ptr,
      dest_public_key.getPtr(),
      amount,
      fee_per_gram,
      utf8.encode(message)
    );
    dest_public_key.destroy();
    return result;
  }

  applyEncryption(passphrase) {
    InterfaceFFI.walletApplyEncryption(
      this.#wallet_ptr,
      utf8.encode(passphrase)
    );
  }

  getCompletedTransactions() {
    let list_ptr = InterfaceFFI.walletGetCompletedTransactions(
      this.#wallet_ptr
    );
    return new CompletedTransactions(list_ptr);
  }

  getInboundTransactions() {
    let list_ptr = InterfaceFFI.walletGetPendingInboundTransactions(
      this.#wallet_ptr
    );
    return new PendingInboundTransactions(list_ptr);
  }

  getOutboundTransactions() {
    let list_ptr = InterfaceFFI.walletGetPendingOutboundTransactions(
      this.#wallet_ptr
    );
    return new PendingOutboundTransactions(list_ptr);
  }

  getContacts() {
    let list_ptr = InterfaceFFI.walletGetContacts(this.#wallet_ptr);
    return new Contacts(list_ptr);
  }

  addContact(alias, pubkey_hex) {
    let public_key = PublicKey.fromHexString(utf8.encode(pubkey_hex));
    let contact = new Contact();
    contact.pointerAssign(
      InterfaceFFI.contactCreate(utf8.encode(alias), public_key.getPtr())
    );
    let result = InterfaceFFI.walletUpsertContact(
      this.#wallet_ptr,
      contact.getPtr()
    );
    contact.destroy();
    public_key.destroy();
    return result;
  }

  removeContact(contact) {
    let result = InterfaceFFI.walletRemoveContact(
      this.#wallet_ptr,
      contact.getPtr()
    );
    contact.destroy();
    return result;
  }

  cancelPendingTransaction(tx_id) {
    return InterfaceFFI.walletCancelPendingTransaction(this.#wallet_ptr, tx_id);
  }

  startUtxoValidation() {
    return InterfaceFFI.walletStartUtxoValidation(this.#wallet_ptr);
  }

  startStxoValidation() {
    return InterfaceFFI.walletStartStxoValidation(this.#wallet_ptr);
  }

  destroy() {
    if (this.#wallet_ptr) {
      InterfaceFFI.walletDestroy(this.#wallet_ptr);
      this.#wallet_ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = Wallet;
