// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
const LivenessData = require("./livenessData");
const TransactionSendStatus = require("./transactionSendStatus");
const FeePerGramStats = require("./feePerGramStats");

class WalletBalance {
  available = 0;
  timeLocked = 0;
  pendingIn = 0;
  pendingOut = 0;
}

class Wallet {
  ptr;
  balance = new WalletBalance();
  livenessData = new Map();
  log_path = "";
  transactionReceived = 0;
  transactionReplyReceived = 0;
  transactionBroadcast = 0;
  transactionMined = 0;
  transactionMinedUnconfirmed = 0;
  transactionFauxConfirmed = 0;
  contactsLivenessDataUpdated = 0;
  transactionFauxUnconfirmed = 0;
  transactionSafMessageReceived = 0;
  transactionCancelled = 0;
  transactionFinalized = 0;
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
  callback_faux_transaction_confirmed;
  callback_faux_transaction_unconfirmed;
  callback_transaction_send_result;
  callback_transaction_cancellation;
  callback_contacts_liveness_data_updated;
  callback_balance_updated;
  callback_transaction_validation_complete;
  callback_saf_message_received;
  callback_connectivity_status;
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
    this.transactionReceived =
      this.transactionReplyReceived =
      this.transactionBroadcast =
      this.transactionMined =
      this.transactionFauxConfirmed =
      this.contactsLivenessDataUpdated =
      this.transactionSafMessageReceived =
      this.transactionCancelled =
      this.transactionMinedUnconfirmed =
      this.transactionFauxUnconfirmed =
      this.transactionFinalized =
        0;
  }

  getCounters() {
    return {
      received: this.transactionReceived,
      replyReceived: this.transactionReplyReceived,
      broadcast: this.transactionBroadcast,
      finalized: this.transactionFinalized,
      minedUnconfirmed: this.transactionMinedUnconfirmed,
      fauxUnconfirmed: this.transactionFauxUnconfirmed,
      cancelled: this.transactionCancelled,
      mined: this.transactionMined,
      fauxConfirmed: this.transactionFauxConfirmed,
      livenessDataUpdated: this.contactsLivenessDataUpdated,
      saf: this.transactionSafMessageReceived,
    };
  }

  getLivenessData() {
    return this.livenessData;
  }

  constructor(
    comms_config_ptr,
    log_path,
    passphrase,
    seed_words_ptr,
    num_rolling_log_file = 50,
    log_size_bytes = 102400,
    network = "localnet"
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
    this.callback_faux_transaction_confirmed =
      InterfaceFFI.createCallbackFauxTransactionConfirmed(
        this.onFauxTransactionConfirmed
      );
    this.callback_faux_transaction_unconfirmed =
      InterfaceFFI.createCallbackFauxTransactionUnconfirmed(
        this.onFauxTransactionUnconfirmed
      );
    this.callback_transaction_send_result =
      InterfaceFFI.createCallbackTransactionSendResult(
        this.onTransactionSendResult
      );
    this.callback_transaction_cancellation =
      InterfaceFFI.createCallbackTransactionCancellation(
        this.onTransactionCancellation
      );
    this.callback_txo_validation_complete =
      InterfaceFFI.createCallbackTxoValidationComplete(
        this.onTxoValidationComplete
      );
    this.callback_contacts_liveness_data_updated =
      InterfaceFFI.createCallbackContactsLivenessUpdated(
        this.onContactsLivenessUpdated
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
    this.callback_connectivity_status =
      InterfaceFFI.createCallbackConnectivityStatus(
        this.onConnectivityStatusChange
      );
    //endregion

    this.transactionReceived = 0;
    this.transactionReplyReceived = 0;
    this.transactionBroadcast = 0;
    this.transactionMined = 0;
    this.transactionFauxConfirmed = 0;
    this.contactsLivenessDataUpdated = 0;
    this.transactionSafMessageReceived = 0;
    this.transactionCancelled = 0;
    this.transactionMinedUnconfirmed = 0;
    this.transactionFauxUnconfirmed = 0;
    this.transactionFinalized = 0;
    this.recoveryFinished = true;
    let sanitize = null;
    let words = null;
    if (passphrase) {
      sanitize = utf8.encode(passphrase);
    }
    if (seed_words_ptr) {
      words = seed_words_ptr;
    }
    if (!network) {
      network = "localnet";
    }

    this.log_path = log_path;
    this.ptr = InterfaceFFI.walletCreate(
      comms_config_ptr,
      utf8.encode(this.log_path), //`${this.baseDir}/log/wallet.log`,
      num_rolling_log_file,
      log_size_bytes,
      sanitize,
      words,
      utf8.encode(network),
      this.callback_received_transaction,
      this.callback_received_transaction_reply,
      this.callback_received_finalized_transaction,
      this.callback_transaction_broadcast,
      this.callback_transaction_mined,
      this.callback_transaction_mined_unconfirmed,
      this.callback_faux_transaction_confirmed,
      this.callback_faux_transaction_unconfirmed,
      this.callback_transaction_send_result,
      this.callback_transaction_cancellation,
      this.callback_txo_validation_complete,
      this.callback_contacts_liveness_data_updated,
      this.callback_balance_updated,
      this.callback_transaction_validation_complete,
      this.callback_saf_message_received,
      this.callback_connectivity_status
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
    this.transactionReceived += 1;
  };

  onReceivedTransactionReply = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received reply for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.transactionReplyReceived += 1;
  };

  onReceivedFinalizedTransaction = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} received finalization for Transaction with txID ${tx.getTransactionID()}.`
    );
    tx.destroy();
    this.transactionFinalized += 1;
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
    this.transactionMinedUnconfirmed += 1;
  };

  onFauxTransactionConfirmed = (ptr) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Faux transaction with txID ${tx.getTransactionID()} was confirmed.`
    );
    tx.destroy();
    this.transactionFauxConfirmed += 1;
  };

  onFauxTransactionUnconfirmed = (ptr, confirmations) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Faux transaction with txID ${tx.getTransactionID()} is unconfirmed with ${confirmations} confirmations.`
    );
    tx.destroy();
    this.transactionFauxUnconfirmed += 1;
  };

  onTransactionCancellation = (ptr, reason) => {
    let tx = new CompletedTransaction();
    tx.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} Transaction with txID ${tx.getTransactionID()} was cancelled with reason code ${reason}.`
    );
    tx.destroy();
    this.transactionCancelled += 1;
  };

  onTransactionSendResult = (id, ptr) => {
    let status = new TransactionSendStatus(ptr);
    status.pointerAssign(ptr);
    console.log(
      `${new Date().toISOString()} callbackTransactionSendResult(${id}: (${status.getSendStatus()}))`
    );
    status.destroy();
  };

  onTxoValidationComplete = (request_key, validation_results) => {
    console.log(
      `${new Date().toISOString()} callbackTxoValidationComplete(${request_key},${validation_results})`
    );
    this.txo_validation_complete = true;
    this.txo_validation_result = validation_results;
  };

  onContactsLivenessUpdated = (ptr) => {
    let data = new LivenessData(ptr);
    data.pointerAssign(ptr);
    const public_key = data.getPublicKey();
    this.addLivenessData(
      public_key,
      data.getLatency(),
      data.getLastSeen(),
      data.getMessageType(),
      data.getOnlineStatus()
    );
    console.log(
      `${new Date().toISOString()} callbackContactsLivenessUpdated: received ${
        this.livenessData.get(public_key).message_type
      } from contact ${public_key} with latency ${
        this.livenessData.get(public_key).latency
      } at ${this.livenessData.get(public_key).last_seen} and is ${
        this.livenessData.get(public_key).online_status
      }`
    );
    this.contactsLivenessDataUpdated += 1;
    data.destroy();
  };

  addLivenessData(public_key, latency, last_seen, message_type, online_status) {
    let data = {
      latency: latency,
      last_seen: last_seen,
      message_type: message_type,
      online_status: online_status,
    };
    this.livenessData.set(public_key, data);
  }

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
    this.transactionSafMessageReceived += 1;
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

  onConnectivityStatusChange = (status) => {
    console.log("Connectivity Status Changed to ", status);
  };

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

  listConnectedPublicKeys() {
    return InterfaceFFI.commsListConnectedPublicKeys(this.ptr);
  }

  getFeePerGramStats(count) {
    return new FeePerGramStats(
      InterfaceFFI.walletGetFeePerGramStats(this.ptr, count)
    );
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
        this.callback_faux_transaction_confirmed =
        this.callback_faux_transaction_unconfirmed =
        this.callback_transaction_send_result =
        this.callback_transaction_cancellation =
        this.callback_txo_validation_complete =
        this.callback_contacts_liveness_data_updated =
        this.callback_balance_updated =
        this.callback_transaction_validation_complete =
        this.callback_saf_message_received =
        this.recoveryProgressCallback =
        this.callback_connectivity_status =
          undefined; // clear callback function pointers
    }
  }
}

module.exports = Wallet;
