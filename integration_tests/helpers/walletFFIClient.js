const WalletFFI = require("./walletFFI");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { expect } = require("chai");

class WalletFFIClient {
  #name;
  #wallet;
  #comms_config;
  #port;

  constructor(name) {
    this.#wallet = null;
    this.#name = name;
    this.baseDir = "";
  }

  static async #convertPubkeyToHex(public_key) {
    const bytes = await WalletFFI.publicKeyGetBytes(public_key);
    const length = await WalletFFI.byteVectorGetLength(bytes);
    const public_key_hex = (
      await Promise.all(
        Array(length)
          .fill()
          .map((_, i) => WalletFFI.byteVectorGetAt(bytes, i))
      )
    )
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
    await WalletFFI.byteVectorDestroy(bytes);
    return public_key_hex;
  }

  static async Init() {
    await WalletFFI.Init();
  }

  async startNew() {
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
    this.#wallet = await WalletFFI.walletCreate(
      this.#comms_config,
      `${this.baseDir}/log/wallet.log`,
      50,
      102400,
      WalletFFI.NULL,
      WalletFFI.NULL
    );
  }

  async stop() {
    await WalletFFI.walletDestroy(this.#wallet);
  }

  async getPublicKey() {
    const public_key = await WalletFFI.walletGetPublicKey(this.#wallet);
    const pubkey_hex = await WalletFFIClient.#convertPubkeyToHex(public_key);
    await WalletFFI.publicKeyDestroy(public_key);
    return pubkey_hex;
  }

  async getEmojiId() {
    const public_key = await WalletFFI.walletGetPublicKey(this.#wallet);
    const emoji_id = await WalletFFI.publicKeyToEmojiId(public_key);
    const result = emoji_id.readCString();
    await WalletFFI.stringDestroy(emoji_id);
    await WalletFFI.publicKeyDestroy(public_key);
    return result;
  }

  async getBalance() {
    return await WalletFFI.walletGetAvailableBalance(this.#wallet);
  }

  async addBaseNodePeer(public_key_hex, address) {
    const public_key = await WalletFFI.publicKeyFromHex(public_key_hex);
    expect(
      await WalletFFI.walletAddBaseNodePeer(this.#wallet, public_key, address)
    ).to.be.true;
    await WalletFFI.publicKeyDestroy(public_key);
  }

  async sendTransaction(destination, amount, fee_per_gram, message) {
    const destinationPubkey = await WalletFFI.publicKeyFromHex(destination);
    const result = await WalletFFI.walletSendTransaction(
      this.#wallet,
      destinationPubkey,
      amount,
      fee_per_gram,
      message
    );
    await WalletFFI.publicKeyDestroy(destinationPubkey);
    return result;
  }

  async applyEncryption(passphrase) {
    await WalletFFI.walletApplyEncryption(this.#wallet, passphrase);
  }

  async getCompletedTransactions() {
    const txs = await WalletFFI.walletGetCompletedTransactions(this.#wallet);
    const length = await WalletFFI.completedTransactionsGetLength(txs);
    let outbound = 0;
    let inbound = 0;
    for (let i = 0; i < length; ++i) {
      const tx = await WalletFFI.completedTransactionsGetAt(txs, i);
      if (await WalletFFI.completedTransactionIsOutbound(tx)) {
        ++outbound;
      } else {
        ++inbound;
      }
      await WalletFFI.completedTransactionDestroy(tx);
    }
    await WalletFFI.completedTransactionsDestroy(txs);
    return [outbound, inbound];
  }

  async getBroadcastTransactionsCount() {
    let broadcast_tx_cnt = 0;
    const txs = await WalletFFI.walletGetPendingOutboundTransactions(
      this.#wallet
    );
    const length = await WalletFFI.pendingOutboundTransactionsGetLength(txs);
    for (let i = 0; i < length; ++i) {
      const tx = await WalletFFI.pendingOutboundTransactionsGetAt(txs, i);
      const status = await WalletFFI.pendingOutboundTransactionGetStatus(tx);
      await WalletFFI.pendingOutboundTransactionDestroy(tx);
      if (status === 1) {
        broadcast_tx_cnt += 1;
      }
    }
    await WalletFFI.pendingOutboundTransactionsDestroy(txs);
    return broadcast_tx_cnt;
  }

  async addContact(alias, pubkey_hex) {
    const public_key = await WalletFFI.publicKeyFromHex(pubkey_hex);
    const contact = await WalletFFI.contactCreate(alias, public_key);
    await WalletFFI.publicKeyDestroy(public_key);
    expect(await WalletFFI.walletUpsertContact(this.#wallet, contact)).to.be
      .true;
    await WalletFFI.contactDestroy(contact);
  }

  async #findContact(lookup_alias) {
    const contacts = await WalletFFI.walletGetContacts(this.#wallet);
    const length = await WalletFFI.contactsGetLength(contacts);
    let contact;
    for (let i = 0; i < length; ++i) {
      contact = await WalletFFI.contactsGetAt(contacts, i);
      const alias = await WalletFFI.contactGetAlias(contact);
      const found = alias.readCString() === lookup_alias;
      await WalletFFI.stringDestroy(alias);
      if (found) {
        break;
      }
      await WalletFFI.contactDestroy(contact);
      contact = undefined;
    }
    await WalletFFI.contactsDestroy(contacts);
    return contact;
  }

  async getContact(alias) {
    const contact = await this.#findContact(alias);
    if (contact) {
      const pubkey = await WalletFFI.contactGetPublicKey(contact);
      const pubkey_hex = await WalletFFIClient.#convertPubkeyToHex(pubkey);
      await WalletFFI.publicKeyDestroy(pubkey);
      await WalletFFI.contactDestroy(contact);
      return pubkey_hex;
    }
  }

  async removeContact(alias) {
    const contact = await this.#findContact(alias);
    if (contact) {
      expect(await WalletFFI.walletRemoveContact(this.#wallet, contact)).to.be
        .true;
      await WalletFFI.contactDestroy(contact);
    }
  }
}

module.exports = WalletFFIClient;
