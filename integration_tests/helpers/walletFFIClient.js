const WalletFFI = require("./walletFFI");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");

class WalletFFIClient {
  #name;
  #wallet;
  #comms_config;
  #port;

  constructor(name) {
    this.#wallet = null;
    this.#name = name;
  }

  static async Init() {
    await WalletFFI.Init();
  }

  async startNew() {
    this.#port = await getFreePort(19000, 25000);
    const name = `WalletFFI${this.#port}-${this.#name}`;
    const baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${name}`;
    const tcp = WalletFFI.transportTcpCreate(`/ip4/0.0.0.0/tcp/${this.#port}`);
    this.#comms_config = WalletFFI.commsConfigCreate(
      `/ip4/0.0.0.0/tcp/${this.#port}`,
      tcp,
      "wallet.dat",
      baseDir,
      30,
      600
    );
    this.#wallet = WalletFFI.walletCreate(
      this.#comms_config,
      `${baseDir}/logs/wallet.log`,
      5,
      10240,
      WalletFFI.NULL,
      WalletFFI.NULL
    );
  }

  stop() {
    WalletFFI.walletDestroy(this.#wallet);
  }

  getPublicKey() {
    const public_key = WalletFFI.walletGetPublicKey(this.#wallet);
    const bytes = WalletFFI.publicKeyGetBytes(public_key);
    const length = WalletFFI.byteVectorGetLength(bytes);
    let public_key_array = Array(length)
      .fill()
      .map((_, i) => WalletFFI.byteVectorGetAt(bytes, i));
    WalletFFI.byteVectorDestroy(bytes);
    WalletFFI.publicKeyDestroy(public_key);
    return public_key_array;
  }

  getEmojiId() {
    const public_key = WalletFFI.walletGetPublicKey(this.#wallet);
    let emoji_id = WalletFFI.publicKeyToEmojiId(public_key);
    WalletFFI.publicKeyDestroy(public_key);
    return emoji_id;
  }
}

module.exports = WalletFFIClient;
