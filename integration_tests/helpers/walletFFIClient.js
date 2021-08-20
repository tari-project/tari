const WalletFFI = require("./ffi/walletFFI");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { expect } = require("chai");
const PublicKey = require("./ffi/publicKey");
const CompletedTransactions = require("./ffi/completedTransactions");
const PendingOutboundTransactions = require("./ffi/pendingOutboundTransactions");
const Contact = require("./ffi/contact");
const Contacts = require("./ffi/contacts");
const SeedWords = require("./ffi/seedWords");

class WalletFFIClient {
  #name;
  #wallet;
  #comms_config;
  #port;
  #callback_received_transaction;
  #callback_received_transaction_reply;
  #callback_received_finalized_transaction;
  #callback_transaction_broadcast;
  #callback_transaction_mined;
  #callback_transaction_mined_unconfirmed;
  #callback_direct_send_result;
  #callback_store_and_forward_send_result;
  #callback_transaction_cancellation;
  #callback_utxo_validation_complete;
  #callback_stxo_validation_complete;
  #callback_invalid_txo_validation_complete;
  #callback_transaction_validation_complete;
  #callback_saf_message_received;
  #recovery_progress_callback;

  #callbackReceivedTransaction = (..._args) => {
    console.log(`${new Date().toISOString()} callbackReceivedTransaction`);
    this.receivedTransaction += 1;
  };
  #callbackReceivedTransactionReply = (..._args) => {
    console.log(`${new Date().toISOString()} callbackReceivedTransactionReply`);
    this.receivedTransactionReply += 1;
  };
  #callbackReceivedFinalizedTransaction = (..._args) => {
    console.log(
      `${new Date().toISOString()} callbackReceivedFinalizedTransaction`
    );
  };
  #callbackTransactionBroadcast = (..._args) => {
    console.log(`${new Date().toISOString()} callbackTransactionBroadcast`);
    this.transactionBroadcast += 1;
  };
  #callbackTransactionMined = (..._args) => {
    console.log(`${new Date().toISOString()} callbackTransactionMined`);
    this.transactionMined += 1;
  };
  #callbackTransactionMinedUnconfirmed = (..._args) => {
    console.log(
      `${new Date().toISOString()} callbackTransactionMinedUnconfirmed`
    );
  };
  #callbackDirectSendResult = (..._args) => {
    console.log(`${new Date().toISOString()} callbackDirectSendResult`);
  };
  #callbackStoreAndForwardSendResult = (..._args) => {
    console.log(
      `${new Date().toISOString()} callbackStoreAndForwardSendResult`
    );
  };
  #callbackTransactionCancellation = (..._args) => {
    console.log(`${new Date().toISOString()} callbackTransactionCancellation`);
  };
  #callbackUtxoValidationComplete = (_request_key, validation_results) => {
    console.log(`${new Date().toISOString()} callbackUtxoValidationComplete`);
    this.utxo_validation_complete = true;
    this.utxo_validation_result = validation_results;
  };
  #callbackStxoValidationComplete = (_request_key, validation_results) => {
    console.log(`${new Date().toISOString()} callbackStxoValidationComplete`);
    this.stxo_validation_complete = true;
    this.stxo_validation_result = validation_results;
  };
  #callbackInvalidTxoValidationComplete = (..._args) => {
    console.log(
      `${new Date().toISOString()} callbackInvalidTxoValidationComplete`
    );
  };
  #callbackTransactionValidationComplete = (..._args) => {
    console.log(
      `${new Date().toISOString()} callbackTransactionValidationComplete`
    );
  };
  #callbackSafMessageReceived = (..._args) => {
    console.log(`${new Date().toISOString()} callbackSafMessageReceived`);
  };
  #recoveryProgressCallback = (a, b, c) => {
    console.log(`${new Date().toISOString()} recoveryProgressCallback`);
    if (a == 3)
      // Progress
      this.recoveryProgress = [b, c];
    if (a == 4)
      // Completed
      this.recoveryInProgress = false;
  };

  clearCallbackCounters() {
    this.receivedTransaction =
      this.receivedTransactionReply =
      this.transactionBroadcast =
      this.transactionMined =
        0;
  }

  constructor(name) {
    this.#wallet = null;
    this.#name = name;
    this.baseDir = "";
    this.clearCallbackCounters();

    // Create the ffi callbacks
    this.#callback_received_transaction =
      WalletFFI.createCallbackReceivedTransaction(
        this.#callbackReceivedTransaction
      );
    this.#callback_received_transaction_reply =
      WalletFFI.createCallbackReceivedTransactionReply(
        this.#callbackReceivedTransactionReply
      );
    this.#callback_received_finalized_transaction =
      WalletFFI.createCallbackReceivedFinalizedTransaction(
        this.#callbackReceivedFinalizedTransaction
      );
    this.#callback_transaction_broadcast =
      WalletFFI.createCallbackTransactionBroadcast(
        this.#callbackTransactionBroadcast
      );
    this.#callback_transaction_mined = WalletFFI.createCallbackTransactionMined(
      this.#callbackTransactionMined
    );
    this.#callback_transaction_mined_unconfirmed =
      WalletFFI.createCallbackTransactionMinedUnconfirmed(
        this.#callbackTransactionMinedUnconfirmed
      );
    this.#callback_direct_send_result =
      WalletFFI.createCallbackDirectSendResult(this.#callbackDirectSendResult);
    this.#callback_store_and_forward_send_result =
      WalletFFI.createCallbackStoreAndForwardSendResult(
        this.#callbackStoreAndForwardSendResult
      );
    this.#callback_transaction_cancellation =
      WalletFFI.createCallbackTransactionCancellation(
        this.#callbackTransactionCancellation
      );
    this.#callback_utxo_validation_complete =
      WalletFFI.createCallbackUtxoValidationComplete(
        this.#callbackUtxoValidationComplete
      );
    this.#callback_stxo_validation_complete =
      WalletFFI.createCallbackStxoValidationComplete(
        this.#callbackStxoValidationComplete
      );
    this.#callback_invalid_txo_validation_complete =
      WalletFFI.createCallbackInvalidTxoValidationComplete(
        this.#callbackInvalidTxoValidationComplete
      );
    this.#callback_transaction_validation_complete =
      WalletFFI.createCallbackTransactionValidationComplete(
        this.#callbackTransactionValidationComplete
      );
    this.#callback_saf_message_received =
      WalletFFI.createCallbackSafMessageReceived(
        this.#callbackSafMessageReceived
      );
    this.#recovery_progress_callback = WalletFFI.createRecoveryProgressCallback(
      this.#recoveryProgressCallback
    );
  }

  static async Init() {
    await WalletFFI.Init();
  }

  async startNew(seed_words_text) {
    this.#port = await getFreePort(19000, 25000);
    const name = `WalletFFI${this.#port}-${this.#name}`;
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${name}`;
    const tcp = await WalletFFI.transportTcpCreate(
      `/ip4/0.0.0.0/tcp/${this.#port}`
    );
    this.#comms_config = await WalletFFI.commsConfigCreate(
      `/ip4/0.0.0.0/tcp/${this.#port}`,
      tcp,
      "wallet.dat",
      this.baseDir,
      30,
      600,
      "localnet"
    );
    await this.start(seed_words_text);
  }

  async start(seed_words_text) {
    let seed_words;
    let seed_words_ptr = WalletFFI.NULL;
    if (seed_words_text) {
      seed_words = await SeedWords.fromString(seed_words_text);
      seed_words_ptr = seed_words.getPtr();
    }
    this.#wallet = await WalletFFI.walletCreate(
      this.#comms_config,
      `${this.baseDir}/log/wallet.log`,
      50,
      102400,
      WalletFFI.NULL,
      seed_words_ptr,
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
    if (seed_words) await seed_words.destroy();
  }

  async startRecovery(base_node_pubkey) {
    const node_pubkey = await PublicKey.fromString(base_node_pubkey);
    expect(
      await WalletFFI.walletStartRecovery(
        this.#wallet,
        node_pubkey.getPtr(),
        this.#recovery_progress_callback
      )
    ).to.be.true;
    node_pubkey.destroy();
    this.recoveryInProgress = true;
  }

  recoveryInProgress() {
    return this.recoveryInProgress;
  }

  async stop() {
    await WalletFFI.walletDestroy(this.#wallet);
  }

  async getPublicKey() {
    const public_key = await PublicKey.fromWallet(this.#wallet);
    const public_key_hex = public_key.getHex();
    public_key.destroy();
    return public_key_hex;
  }

  async getEmojiId() {
    const public_key = await PublicKey.fromWallet(this.#wallet);
    const emoji_id = await public_key.getEmojiId();
    public_key.destroy();
    return emoji_id;
  }

  async getBalance() {
    return await WalletFFI.walletGetAvailableBalance(this.#wallet);
  }

  async addBaseNodePeer(public_key_hex, address) {
    const public_key = await PublicKey.fromString(public_key_hex);
    expect(
      await WalletFFI.walletAddBaseNodePeer(
        this.#wallet,
        public_key.getPtr(),
        address
      )
    ).to.be.true;
    await public_key.destroy();
  }

  async sendTransaction(destination, amount, fee_per_gram, message) {
    const dest_public_key = await PublicKey.fromString(destination);
    const result = await WalletFFI.walletSendTransaction(
      this.#wallet,
      dest_public_key.getPtr(),
      amount,
      fee_per_gram,
      message
    );
    await dest_public_key.destroy();
    return result;
  }

  async applyEncryption(passphrase) {
    await WalletFFI.walletApplyEncryption(this.#wallet, passphrase);
  }

  async getCompletedTransactions() {
    const txs = await CompletedTransactions.fromWallet(this.#wallet);
    const length = await txs.getLength();
    let outbound = 0;
    let inbound = 0;
    for (let i = 0; i < length; ++i) {
      const tx = await txs.getAt(i);
      if (await tx.isOutbound()) {
        ++outbound;
      } else {
        ++inbound;
      }
      tx.destroy();
    }
    txs.destroy();
    return [outbound, inbound];
  }

  async getBroadcastTransactionsCount() {
    let broadcast_tx_cnt = 0;
    const txs = await PendingOutboundTransactions.fromWallet(this.#wallet);
    const length = await txs.getLength();
    for (let i = 0; i < length; ++i) {
      const tx = await txs.getAt(i);
      const status = await tx.getStatus();
      tx.destroy();
      if (status === 1) {
        // Broadcast
        broadcast_tx_cnt += 1;
      }
    }
    await txs.destroy();
    return broadcast_tx_cnt;
  }

  async getOutboundTransactionsCount() {
    let outbound_tx_cnt = 0;
    const txs = await PendingOutboundTransactions.fromWallet(this.#wallet);
    const length = await txs.getLength();
    for (let i = 0; i < length; ++i) {
      const tx = await txs.getAt(i);
      const status = await tx.getStatus();
      if (status === 4) {
        // Pending
        outbound_tx_cnt += 1;
      }
      tx.destroy();
    }
    await txs.destroy();
    return outbound_tx_cnt;
  }

  async addContact(alias, pubkey_hex) {
    const public_key = await PublicKey.fromString(pubkey_hex);
    const contact = new Contact(
      await WalletFFI.contactCreate(alias, public_key.getPtr())
    );
    public_key.destroy();
    expect(await WalletFFI.walletUpsertContact(this.#wallet, contact.getPtr()))
      .to.be.true;
    contact.destroy();
  }

  async #findContact(lookup_alias) {
    const contacts = await Contacts.fromWallet(this.#wallet);
    const length = await contacts.getLength();
    let contact;
    for (let i = 0; i < length; ++i) {
      contact = await contacts.getAt(i);
      const alias = await contact.getAlias();
      const found = alias === lookup_alias;
      if (found) {
        break;
      }
      contact.destroy();
      contact = undefined;
    }
    contacts.destroy();
    return contact;
  }

  async getContact(alias) {
    const contact = await this.#findContact(alias);
    if (contact) {
      const pubkey = await contact.getPubkey();
      const pubkey_hex = pubkey.getHex();
      pubkey.destroy();
      contact.destroy();
      return pubkey_hex;
    }
  }

  async removeContact(alias) {
    const contact = await this.#findContact(alias);
    if (contact) {
      expect(
        await WalletFFI.walletRemoveContact(this.#wallet, contact.getPtr())
      ).to.be.true;
      contact.destroy();
    }
  }

  async identify() {
    return {
      public_key: await this.getPublicKey(),
    };
  }

  async cancelAllOutboundTransactions() {
    const txs = await PendingOutboundTransactions.fromWallet(this.#wallet);
    const length = await txs.getLength();
    let cancelled = 0;
    for (let i = 0; i < length; ++i) {
      const tx = await txs.getAt(i);
      if (
        await WalletFFI.walletCancelPendingTransaction(
          this.#wallet,
          await tx.getTransactionId()
        )
      ) {
        ++cancelled;
      }
      tx.destroy();
    }
    txs.destroy();
    return cancelled;
  }

  startUtxoValidation() {
    this.utxo_validation_complete = false;
    return WalletFFI.walletStartUtxoValidation(this.#wallet);
  }

  startStxoValidation() {
    this.stxo_validation_complete = false;
    return WalletFFI.walletStartStxoValidation(this.#wallet);
  }
}

module.exports = WalletFFIClient;
