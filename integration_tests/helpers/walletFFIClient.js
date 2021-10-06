const SeedWords = require("./ffi/seedWords");
const TransportType = require("./ffi/transportType");
const CommsConfig = require("./ffi/commsConfig");
const Wallet = require("./ffi/wallet");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");

class WalletFFIClient {
  name;
  wallet;
  comms_config;
  transport;
  seed_words;
  pass_phrase;
  port;
  baseDir = "";

  constructor(name) {
    this.name = name;
  }

  async startNew(seed_words_text, pass_phrase) {
    this.port = await getFreePort(19000, 25000);
    const name = `WalletFFI${this.port}-${this.name}`;
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${name}`;
    this.transport = TransportType.createTCP(`/ip4/0.0.0.0/tcp/${this.port}`);
    this.comms_config = new CommsConfig(
      `/ip4/0.0.0.0/tcp/${this.port}`,
      this.transport.getPtr(),
      "wallet.dat",
      this.baseDir,
      30,
      600,
      "localnet"
    );
    this.start(seed_words_text, pass_phrase);
  }

  async restart(seed_words_text, pass_phrase) {
    this.transport = TransportType.createTCP(`/ip4/0.0.0.0/tcp/${this.port}`);
    this.comms_config = new CommsConfig(
      `/ip4/0.0.0.0/tcp/${this.port}`,
      this.transport.getPtr(),
      "wallet.dat",
      this.baseDir,
      30,
      600,
      "localnet"
    );
    this.start(seed_words_text, pass_phrase);
  }

  getTxoValidationStatus() {
    return this.wallet.getTxoValidationStatus();
  }

  getTxValidationStatus() {
    return this.wallet.getTxValidationStatus();
  }

  identify() {
    return this.wallet.getPublicKey();
  }

  identifyEmoji() {
    return this.wallet.getEmojiId();
  }

  getBalance() {
    return this.wallet.getBalance();
  }

  addBaseNodePeer(public_key_hex, address) {
    return this.wallet.addBaseNodePeer(public_key_hex, address);
  }

  addContact(alias, pubkey_hex) {
    return this.wallet.addContact(alias, pubkey_hex);
  }

  getContactList() {
    return this.wallet.getContacts();
  }

  getCompletedTxs() {
    return this.wallet.getCompletedTransactions();
  }

  getInboundTxs() {
    return this.wallet.getInboundTransactions();
  }

  getOutboundTxs() {
    return this.wallet.getOutboundTransactions();
  }

  removeContact(contact) {
    return this.wallet.removeContact(contact);
  }

  startRecovery(base_node_pubkey) {
    this.wallet.startRecovery(base_node_pubkey);
  }

  checkRecoveryInProgress() {
    return this.wallet.recoveryInProgress();
  }

  applyEncryption(passphrase) {
    this.wallet.applyEncryption(passphrase);
  }

  startTxoValidation() {
    this.wallet.startTxoValidation();
  }

  startTxValidation() {
    this.wallet.startTxValidation();
  }

  getCounters() {
    return this.wallet.getCounters();
  }
  resetCounters() {
    this.wallet.clearCallbackCounters();
  }

  sendTransaction(destination, amount, fee_per_gram, message) {
    return this.wallet.sendTransaction(
      destination,
      amount,
      fee_per_gram,
      message
    );
  }

  start(
    seed_words_text,
    pass_phrase,
    rolling_log_files = 50,
    byte_size_per_log = 102400
  ) {
    this.pass_phrase = pass_phrase;
    if (seed_words_text) {
      let seed_words = SeedWords.fromText(seed_words_text);
      this.seed_words = seed_words;
    }

    let log_path = `${this.baseDir}/log/wallet.log`;
    this.wallet = new Wallet(
      this.comms_config.getPtr(),
      log_path,
      this.pass_phrase,
      this.seed_words ? this.seed_words.getPtr() : null,
      rolling_log_files,
      byte_size_per_log
    );
  }

  getOutboundTransactions() {
    return this.wallet.getOutboundTransactions();
  }

  cancelPendingTransaction(tx_id) {
    return this.wallet.cancelPendingTransaction(tx_id);
  }

  stop() {
    if (this.wallet) {
      this.wallet.destroy();
    }
    if (this.comms_config) {
      this.comms_config.destroy();
    }
    if (this.transport) {
      this.transport.destroy();
    }
    if (this.seed_words) {
      this.seed_words.destroy();
    }
  }
}

module.exports = WalletFFIClient;
