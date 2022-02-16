const SeedWords = require("./ffi/seedWords");
const TransportType = require("./ffi/transportType");
const CommsConfig = require("./ffi/commsConfig");
const Wallet = require("./ffi/wallet");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { sleep } = require("./util");

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

  pollBalance() {
    return this.wallet.pollBalance();
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

  getMnemonicWordListForLanguage(language) {
    return SeedWords.getMnemonicWordListForLanguage(language);
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

  listConnectedPublicKeys() {
    this.wallet.listConnectedPublicKeys();
  }

  getCounters() {
    return this.wallet.getCounters();
  }
  resetCounters() {
    if (this.wallet) {
      this.wallet.clearCallbackCounters();
    }
  }

  sendTransaction(destination, amount, fee_per_gram, message, one_sided) {
    return this.wallet.sendTransaction(
      destination,
      amount,
      fee_per_gram,
      message,
      one_sided
    );
  }

  start(
    seed_words_text,
    pass_phrase,
    rolling_log_files = 50,
    byte_size_per_log = 1048576
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

  getCancelledTransactions() {
    return this.wallet.walletGetCancelledTransactions();
  }

  cancelPendingTransaction(tx_id) {
    return this.wallet.cancelPendingTransaction(tx_id);
  }

  async stop() {
    if (this.comms_config) {
      //      console.log("walletFFI destroy comms_config ...");
      await this.comms_config.destroy();
      this.comms_config = undefined;
      //      console.log("walletFFI destroy comms_config ... done!");
      await sleep(100);
    }
    if (this.transport) {
      //      console.log("walletFFI destroy transport ...");
      await this.transport.destroy();
      this.transport = undefined;
      //      console.log("walletFFI destroy transport ... done!");
      await sleep(100);
    }
    if (this.seed_words) {
      //      console.log("walletFFI destroy seed_words ...");
      await this.seed_words.destroy();
      this.seed_words = undefined;
      //      console.log("walletFFI destroy seed_words ... done!");
    }
    if (this.wallet) {
      //      console.log("walletFFI destroy wallet ...");
      await this.wallet.destroy();
      this.wallet = undefined;
      //      console.log("walletFFI destroy wallet ... done!");
    }
  }
}

module.exports = WalletFFIClient;
