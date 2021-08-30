/**
 * This library was AUTO-GENERATED. Do not modify manually!
 */

const { expect } = require("chai");
const ffi = require("ffi-napi");
const ref = require("ref-napi");
const dateFormat = require("dateformat");
const { spawn } = require("child_process");
const fs = require("fs");

class InterfaceFFI {
  //region Compile
  static compile() {
    return new Promise((resolve, _reject) => {
      const cmd = "cargo";
      const args = [
        "build",
        "--release",
        "--package",
        "tari_wallet_ffi",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out",
      ];
      const baseDir = `./temp/base_nodes/${dateFormat(
        new Date(),
        "yyyymmddHHMM"
      )}/WalletFFI-compile`;
      if (!fs.existsSync(baseDir)) {
        fs.mkdirSync(baseDir, { recursive: true });
        fs.mkdirSync(baseDir + "/log", { recursive: true });
      }
      const ps = spawn(cmd, args, {
        cwd: baseDir,
        env: { ...process.env },
      });
      ps.on("close", (_code) => {
        resolve(ps);
      });
      ps.stderr.on("data", (data) => {
        console.log("stderr : ", data.toString());
      });
      ps.on("error", (error) => {
        console.log("error : ", error.toString());
      });
      expect(ps.error).to.be.an("undefined");
      this.#ps = ps;
    });
  }
  //endregion

  //region Interface
  static #fn;

  static #loaded = false;
  static #ps = null;

  static async Init() {
    if (this.#loaded) {
      return;
    }

    this.#loaded = true;
    await this.compile();
    const outputProcess = `${process.cwd()}/temp/out/${
      process.platform === "win32" ? "" : "lib"
    }tari_wallet_ffi`;

    // Load the library
    this.#fn = ffi.Library(outputProcess, {
      transport_memory_create: ["pointer", ["void"]],
      transport_tcp_create: ["pointer", ["string", "int*"]],
      transport_tor_create: [
        "pointer",
        ["string", "pointer", "ushort", "string", "string", "int*"],
      ],
      transport_memory_get_address: ["char*", ["pointer", "int*"]],
      transport_type_destroy: ["void", ["pointer"]],
      string_destroy: ["void", ["string"]],
      byte_vector_create: ["pointer", ["uchar*", "uint", "int*"]],
      byte_vector_get_at: ["uchar", ["pointer", "uint", "int*"]],
      byte_vector_get_length: ["uint", ["pointer", "int*"]],
      byte_vector_destroy: ["void", ["pointer"]],
      public_key_create: ["pointer", ["pointer", "int*"]],
      public_key_get_bytes: ["pointer", ["pointer", "int*"]],
      public_key_from_private_key: ["pointer", ["pointer", "int*"]],
      public_key_from_hex: ["pointer", ["string", "int*"]],
      public_key_destroy: ["void", ["pointer"]],
      public_key_to_emoji_id: ["char*", ["pointer", "int*"]],
      emoji_id_to_public_key: ["pointer", ["string", "int*"]],
      private_key_create: ["pointer", ["pointer", "int*"]],
      private_key_generate: ["pointer", ["void"]],
      private_key_get_bytes: ["pointer", ["pointer", "int*"]],
      private_key_from_hex: ["pointer", ["string", "int*"]],
      private_key_destroy: ["void", ["pointer"]],
      seed_words_create: ["pointer", ["void"]],
      seed_words_get_length: ["uint", ["pointer", "int*"]],
      seed_words_get_at: ["char*", ["pointer", "uint", "int*"]],
      seed_words_push_word: ["uchar", ["pointer", "string", "int*"]],
      seed_words_destroy: ["void", ["pointer"]],
      contact_create: ["pointer", ["string", "pointer", "int*"]],
      contact_get_alias: ["char*", ["pointer", "int*"]],
      contact_get_public_key: ["pointer", ["pointer", "int*"]],
      contact_destroy: ["void", ["pointer"]],
      contacts_get_length: ["uint", ["pointer", "int*"]],
      contacts_get_at: ["pointer", ["pointer", "uint", "int*"]],
      contacts_destroy: ["void", ["pointer"]],
      completed_transaction_get_destination_public_key: [
        "pointer",
        ["pointer", "int*"],
      ],
      completed_transaction_get_source_public_key: [
        "pointer",
        ["pointer", "int*"],
      ],
      completed_transaction_get_amount: ["uint64", ["pointer", "int*"]],
      completed_transaction_get_fee: ["uint64", ["pointer", "int*"]],
      completed_transaction_get_message: ["char*", ["pointer", "int*"]],
      completed_transaction_get_status: ["int", ["pointer", "int*"]],
      completed_transaction_get_transaction_id: ["uint64", ["pointer", "int*"]],
      completed_transaction_get_timestamp: ["uint64", ["pointer", "int*"]],
      completed_transaction_is_valid: ["bool", ["pointer", "int*"]],
      completed_transaction_is_outbound: ["bool", ["pointer", "int*"]],
      completed_transaction_get_confirmations: ["uint64", ["pointer", "int*"]],
      completed_transaction_destroy: ["void", ["pointer"]],
      //completed_transaction_get_excess: [
      //this.tari_excess_ptr,
      //  [this.tari_completed_transaction_ptr, "int*"],
      //],
      //completed_transaction_get_public_nonce: [
      // this.tari_excess_public_nonce_ptr,
      //  [this.tari_completed_transaction_ptr, "int*"],
      //],
      //completed_transaction_get_signature: [
      //  this.tari_excess_signature_ptr,
      //  [this.tari_completed_transaction_ptr, "int*"],
      //],
      // excess_destroy: ["void", [this.tari_excess_ptr]],
      // nonce_destroy: ["void", [this.tari_excess_public_nonce_ptr]],
      // signature_destroy: ["void", [this.tari_excess_signature_ptr]],
      completed_transactions_get_length: ["uint", ["pointer", "int*"]],
      completed_transactions_get_at: ["pointer", ["pointer", "uint", "int*"]],
      completed_transactions_destroy: ["void", ["pointer"]],
      pending_outbound_transaction_get_transaction_id: [
        "uint64",
        ["pointer", "int*"],
      ],
      pending_outbound_transaction_get_destination_public_key: [
        "pointer",
        ["pointer", "int*"],
      ],
      pending_outbound_transaction_get_amount: ["uint64", ["pointer", "int*"]],
      pending_outbound_transaction_get_fee: ["uint64", ["pointer", "int*"]],
      pending_outbound_transaction_get_message: ["char*", ["pointer", "int*"]],
      pending_outbound_transaction_get_timestamp: [
        "uint64",
        ["pointer", "int*"],
      ],
      pending_outbound_transaction_get_status: ["int", ["pointer", "int*"]],
      pending_outbound_transaction_destroy: ["void", ["pointer"]],
      pending_outbound_transactions_get_length: ["uint", ["pointer", "int*"]],
      pending_outbound_transactions_get_at: [
        "pointer",
        ["pointer", "uint", "int*"],
      ],
      pending_outbound_transactions_destroy: ["void", ["pointer"]],
      pending_inbound_transaction_get_transaction_id: [
        "uint64",
        ["pointer", "int*"],
      ],
      pending_inbound_transaction_get_source_public_key: [
        "pointer",
        ["pointer", "int*"],
      ],
      pending_inbound_transaction_get_message: ["char*", ["pointer", "int*"]],
      pending_inbound_transaction_get_amount: ["uint64", ["pointer", "int*"]],
      pending_inbound_transaction_get_timestamp: [
        "uint64",
        ["pointer", "int*"],
      ],
      pending_inbound_transaction_get_status: ["int", ["pointer", "int*"]],
      pending_inbound_transaction_destroy: ["void", ["pointer"]],
      pending_inbound_transactions_get_length: ["uint", ["pointer", "int*"]],
      pending_inbound_transactions_get_at: [
        "pointer",
        ["pointer", "uint", "int*"],
      ],
      pending_inbound_transactions_destroy: ["void", ["pointer"]],
      comms_config_create: [
        "pointer",
        [
          "string",
          "pointer",
          "string",
          "string",
          "uint64",
          "uint64",
          "string",
          "int*",
        ],
      ],
      comms_config_destroy: ["void", ["pointer"]],
      wallet_create: [
        "pointer",
        [
          "pointer",
          "string",
          "uint",
          "uint",
          "string",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "pointer",
          "bool*",
          "int*",
        ],
      ],
      wallet_sign_message: ["char*", ["pointer", "string", "int*"]],
      wallet_verify_message_signature: [
        "bool",
        ["pointer", "pointer", "string", "string", "int*"],
      ],
      wallet_add_base_node_peer: [
        "bool",
        ["pointer", "pointer", "string", "int*"],
      ],
      wallet_upsert_contact: ["bool", ["pointer", "pointer", "int*"]],
      wallet_remove_contact: ["bool", ["pointer", "pointer", "int*"]],
      wallet_get_available_balance: ["uint64", ["pointer", "int*"]],
      wallet_get_pending_incoming_balance: ["uint64", ["pointer", "int*"]],
      wallet_get_pending_outgoing_balance: ["uint64", ["pointer", "int*"]],
      wallet_get_fee_estimate: [
        "uint64",
        ["pointer", "uint64", "uint64", "uint64", "uint64", "int*"],
      ],
      wallet_get_num_confirmations_required: ["uint64", ["pointer", "int*"]],
      wallet_set_num_confirmations_required: [
        "void",
        ["pointer", "uint64", "int*"],
      ],
      wallet_send_transaction: [
        "uint64",
        ["pointer", "pointer", "uint64", "uint64", "string", "int*"],
      ],
      wallet_get_contacts: ["pointer", ["pointer", "int*"]],
      wallet_get_completed_transactions: ["pointer", ["pointer", "int*"]],
      wallet_get_pending_outbound_transactions: [
        "pointer",
        ["pointer", "int*"],
      ],
      wallet_get_public_key: ["pointer", ["pointer", "int*"]],
      wallet_get_pending_inbound_transactions: ["pointer", ["pointer", "int*"]],
      wallet_get_cancelled_transactions: ["pointer", ["pointer", "int*"]],
      wallet_get_completed_transaction_by_id: [
        "pointer",
        ["pointer", "uint64", "int*"],
      ],
      wallet_get_pending_outbound_transaction_by_id: [
        "pointer",
        ["pointer", "uint64", "int*"],
      ],
      wallet_get_pending_inbound_transaction_by_id: [
        "pointer",
        ["pointer", "uint64", "int*"],
      ],
      wallet_get_cancelled_transaction_by_id: [
        "pointer",
        ["pointer", "uint64", "int*"],
      ],
      wallet_import_utxo: [
        "uint64",
        ["pointer", "uint64", "pointer", "pointer", "string", "int*"],
      ],
      wallet_start_utxo_validation: ["uint64", ["pointer", "int*"]],
      wallet_start_stxo_validation: ["uint64", ["pointer", "int*"]],
      wallet_start_invalid_txo_validation: ["uint64", ["pointer", "int*"]],
      wallet_start_transaction_validation: ["uint64", ["pointer", "int*"]],
      wallet_restart_transaction_broadcast: ["bool", ["pointer", "int*"]],
      wallet_set_low_power_mode: ["void", ["pointer", "int*"]],
      wallet_set_normal_power_mode: ["void", ["pointer", "int*"]],
      wallet_cancel_pending_transaction: [
        "bool",
        ["pointer", "uint64", "int*"],
      ],
      wallet_coin_split: [
        "uint64",
        ["pointer", "uint64", "uint64", "uint64", "string", "uint64", "int*"],
      ],
      wallet_get_seed_words: ["pointer", ["pointer", "int*"]],
      wallet_apply_encryption: ["void", ["pointer", "string", "int*"]],
      wallet_remove_encryption: ["void", ["pointer", "int*"]],
      wallet_set_key_value: ["bool", ["pointer", "string", "string", "int*"]],
      wallet_get_value: ["char*", ["pointer", "string", "int*"]],
      wallet_clear_value: ["bool", ["pointer", "string", "int*"]],
      wallet_is_recovery_in_progress: ["bool", ["pointer", "int*"]],
      wallet_start_recovery: [
        "bool",
        ["pointer", "pointer", "pointer", "int*"],
      ],
      wallet_destroy: ["void", ["pointer"]],
      file_partial_backup: ["void", ["string", "string", "int*"]],
      log_debug_message: ["void", ["string"]],
      get_emoji_set: ["pointer", ["void"]],
      emoji_set_destroy: ["void", ["pointer"]],
      emoji_set_get_at: ["pointer", ["pointer", "uint", "int*"]],
      emoji_set_get_length: ["uint", ["pointer", "int*"]],
    });
  }
  //endregion

  static checkErrorResult(error, error_name) {
    expect(error.deref()).to.equal(0, `Error in ${error_name}`);
  }

  //region Helpers
  static initError() {
    let error = Buffer.alloc(4);
    error.writeInt32LE(-1, 0);
    error.type = ref.types.int;
    return error;
  }

  static initBool() {
    let boolean = ref.alloc(ref.types.bool);
    return boolean;
  }

  static filePartialBackup(original_file_path, backup_file_path) {
    let error = this.initError();
    let result = this.#fn.file_partial_backup(
      original_file_path,
      backup_file_path,
      error
    );
    this.checkErrorResult(error, `filePartialBackup`);
    return result;
  }

  static logDebugMessage(msg) {
    this.#fn.log_debug_message(msg);
  }
  //endregion

  //region String
  static stringDestroy(s) {
    this.#fn.string_destroy(s);
  }
  //endregion

  // region ByteVector
  static byteVectorCreate(byte_array, element_count) {
    let error = this.initError();
    let result = this.#fn.byte_vector_create(byte_array, element_count, error);
    this.checkErrorResult(error, `byteVectorCreate`);
    return result;
  }

  static byteVectorGetAt(ptr, i) {
    let error = this.initError();
    let result = this.#fn.byte_vector_get_at(ptr, i, error);
    this.checkErrorResult(error, `byteVectorGetAt`);
    return result;
  }

  static byteVectorGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.byte_vector_get_length(ptr, error);
    this.checkErrorResult(error, `byteVectorGetLength`);
    return result;
  }

  static byteVectorDestroy(ptr) {
    this.#fn.byte_vector_destroy(ptr);
  }
  //endregion

  //region PrivateKey
  static privateKeyCreate(ptr) {
    let error = this.initError();
    let result = this.#fn.private_key_create(ptr, error);
    this.checkErrorResult(error, `privateKeyCreate`);
    return result;
  }

  static privateKeyGenerate() {
    return this.#fn.private_key_generate();
  }

  static privateKeyGetBytes(ptr) {
    let error = this.initError();
    let result = this.#fn.private_key_get_bytes(ptr, error);
    this.checkErrorResult(error, "privateKeyGetBytes");
    return result;
  }

  static privateKeyFromHex(hex) {
    let error = this.initError();
    let result = this.#fn.private_key_from_hex(hex, error);
    this.checkErrorResult(error, "privateKeyFromHex");
    return result;
  }

  static privateKeyDestroy(ptr) {
    this.#fn.private_key_destroy(ptr);
  }

  //endregion

  //region PublicKey
  static publicKeyCreate(ptr) {
    let error = this.initError();
    let result = this.#fn.public_key_create(ptr, error);
    this.checkErrorResult(error, `publicKeyCreate`);
    return result;
  }

  static publicKeyGetBytes(ptr) {
    let error = this.initError();
    let result = this.#fn.public_key_get_bytes(ptr, error);
    this.checkErrorResult(error, `publicKeyGetBytes`);
    return result;
  }

  static publicKeyFromPrivateKey(ptr) {
    let error = this.initError();
    let result = this.#fn.public_key_from_private_key(ptr, error);
    this.checkErrorResult(error, `publicKeyFromPrivateKey`);
    return result;
  }

  static publicKeyFromHex(hex) {
    let error = this.initError();
    let result = this.#fn.public_key_from_hex(hex, error);
    this.checkErrorResult(error, `publicKeyFromHex`);
    return result;
  }

  static emojiIdToPublicKey(emoji) {
    let error = this.initError();
    let result = this.#fn.emoji_id_to_public_key(emoji, error);
    this.checkErrorResult(error, `emojiIdToPublicKey`);
    return result;
  }

  static publicKeyToEmojiId(ptr) {
    let error = this.initError();
    let result = this.#fn.public_key_to_emoji_id(ptr, error);
    this.checkErrorResult(error, `publicKeyToEmojiId`);
    return result;
  }

  static publicKeyDestroy(ptr) {
    this.#fn.public_key_destroy(ptr);
  }
  //endregion

  //region TransportType
  static transportMemoryCreate() {
    return this.#fn.transport_memory_create();
  }

  static transportTcpCreate(listener_address) {
    let error = this.initError();
    let result = this.#fn.transport_tcp_create(listener_address, error);
    this.checkErrorResult(error, `transportTcpCreate`);
    return result;
  }

  static transportTorCreate(
    control_server_address,
    tor_cookie,
    tor_port,
    socks_username,
    socks_password
  ) {
    let error = this.initError();
    let result = this.#fn.transport_tor_create(
      control_server_address,
      tor_cookie,
      tor_port,
      socks_username,
      socks_password,
      error
    );
    this.checkErrorResult(error, `transportTorCreate`);
    return result;
  }

  static transportMemoryGetAddress(transport) {
    let error = this.initError();
    let result = this.#fn.transport_memory_get_address(transport, error);
    this.checkErrorResult(error, `transportMemoryGetAddress`);
    return result;
  }

  static transportTypeDestroy(transport) {
    this.#fn.transport_type_destroy(transport);
  }
  //endregion

  //region EmojiSet
  static getEmojiSet() {
    return this.#fn.this.#fn.get_emoji_set();
  }

  static emojiSetDestroy(ptr) {
    this.#fn.emoji_set_destroy(ptr);
  }

  static emojiSetGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.emoji_set_get_at(ptr, position, error);
    this.checkErrorResult(error, `emojiSetGetAt`);
    return result;
  }

  static emojiSetGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.emoji_set_get_length(ptr, error);
    this.checkErrorResult(error, `emojiSetGetLength`);
    return result;
  }
  //endregion

  //region SeedWords
  static seedWordsCreate() {
    return this.#fn.seed_words_create();
  }

  static seedWordsGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.seed_words_get_length(ptr, error);
    this.checkErrorResult(error, `emojiSetGetLength`);
    return result;
  }

  static seedWordsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.seed_words_get_at(ptr, position, error);
    this.checkErrorResult(error, `seedWordsGetAt`);
    return result;
  }

  static seedWordsPushWord(ptr, word) {
    let error = this.initError();
    let result = this.#fn.seed_words_push_word(ptr, word, error);
    this.checkErrorResult(error, `seedWordsPushWord`);
    return result;
  }

  static seedWordsDestroy(ptr) {
    this.#fn.seed_words_destroy(ptr);
  }
  //endregion

  //region CommsConfig
  static commsConfigCreate(
    public_address,
    transport,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs,
    network
  ) {
    let error = this.initError();
    let result = this.#fn.comms_config_create(
      public_address,
      transport,
      database_name,
      datastore_path,
      discovery_timeout_in_secs,
      saf_message_duration_in_secs,
      network,
      error
    );
    this.checkErrorResult(error, `commsConfigCreate`);
    return result;
  }

  static commsConfigDestroy(ptr) {
    this.#fn.comms_config_destroy(ptr);
  }
  //endregion

  //region Contact
  static contactCreate(alias, public_key) {
    let error = this.initError();
    let result = this.#fn.contact_create(alias, public_key, error);
    this.checkErrorResult(error, `contactCreate`);
    return result;
  }

  static contactGetAlias(ptr) {
    let error = this.initError();
    let result = this.#fn.contact_get_alias(ptr, error);
    this.checkErrorResult(error, `contactGetAlias`);
    return result;
  }

  static contactGetPublicKey(ptr) {
    let error = this.initError();
    let result = this.#fn.contact_get_public_key(ptr, error);
    this.checkErrorResult(error, `contactGetPublicKey`);
    return result;
  }

  static contactDestroy(ptr) {
    this.#fn.contact_destroy(ptr);
  }
  //endregion

  //region Contacts (List)
  static contactsGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.contacts_get_length(ptr, error);
    this.checkErrorResult(error, `contactsGetLength`);
    return result;
  }

  static contactsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.contacts_get_at(ptr, position, error);
    this.checkErrorResult(error, `contactsGetAt`);
    return result;
  }

  static contactsDestroy(ptr) {
    this.#fn.contacts_destroy(ptr);
  }
  //endregion

  //region CompletedTransaction
  static completedTransactionGetDestinationPublicKey(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_destination_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetDestinationPublicKey`);
    return result;
  }

  static completedTransactionGetSourcePublicKey(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_source_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetSourcePublicKey`);
    return result;
  }

  static completedTransactionGetAmount(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetAmount`);
    return result;
  }

  static completedTransactionGetFee(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_fee(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetFee`);
    return result;
  }

  static completedTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetMessage`);
    return result;
  }

  static completedTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetStatus`);
    return result;
  }

  static completedTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_transaction_id(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetTransactionId`);
    return result;
  }

  static completedTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_timestamp(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetTimestamp`);
    return result;
  }

  static completedTransactionIsValid(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_is_valid(ptr, error);
    this.checkErrorResult(error, `completedTransactionIsValid`);
    return result;
  }

  static completedTransactionIsOutbound(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_is_outbound(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetConfirmations`);
    return result;
  }

  static completedTransactionGetConfirmations(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transaction_get_confirmations(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetConfirmations`);
    return result;
  }

  static completedTransactionDestroy(ptr) {
    this.#fn.completed_transaction_destroy(ptr);
  }

  //endregion

  /*
  //Flagged as design flaw in the FFI lib

  static completedTransactionGetExcess(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_excess.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetExcess")
      )
    );
  }

  static completedTransactionGetPublicNonce(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_public_nonce.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "completedTransactionGetPublicNonce"
        )
      )
    );
  }

  static completedTransactionGetSignature(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_signature.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetSignature")
      )
    );
  }

  static excessDestroy(excess) {
    return new Promise((resolve, reject) =>
      this.#fn.excess_destroy.async(
        excess,
        this.checkAsyncRes(resolve, reject, "excessDestroy")
      )
    );
  }

  static nonceDestroy(nonce) {
    return new Promise((resolve, reject) =>
      this.#fn.nonce_destroy.async(
        nonce,
        this.checkAsyncRes(resolve, reject, "nonceDestroy")
      )
    );
  }

  static signatureDestroy(signature) {
    return new Promise((resolve, reject) =>
      this.#fn.signature_destroy.async(
        signature,
        this.checkAsyncRes(resolve, reject, "signatureDestroy")
      )
    );
  }
  */

  //region CompletedTransactions (List)
  static completedTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.completed_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `contactsGetAt`);
    return result;
  }

  static completedTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.completed_transactions_get_at(ptr, position, error);
    this.checkErrorResult(error, `contactsGetAt`);
    return result;
  }

  static completedTransactionsDestroy(transactions) {
    this.#fn.completed_transactions_destroy(transactions);
  }
  //endregion

  //region PendingOutboundTransaction
  static pendingOutboundTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_transaction_id(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingOutboundTransactionGetTransactionId`);
    return result;
  }

  static pendingOutboundTransactionGetDestinationPublicKey(ptr) {
    let error = this.initError();
    let result =
      this.#fn.pending_outbound_transaction_get_destination_public_key(
        ptr,
        error
      );
    this.checkErrorResult(
      error,
      `pendingOutboundTransactionGetDestinationPublicKey`
    );
    return result;
  }

  static pendingOutboundTransactionGetAmount(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetAmount`);
    return result;
  }

  static pendingOutboundTransactionGetFee(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_fee(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetFee`);
    return result;
  }

  static pendingOutboundTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetMessage`);
    return result;
  }

  static pendingOutboundTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_timestamp(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingOutboundTransactionGetTimestamp`);
    return result;
  }

  static pendingOutboundTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetStatus`);
    return result;
  }

  static pendingOutboundTransactionDestroy(ptr) {
    this.#fn.pending_outbound_transaction_destroy(ptr);
  }
  //endregion

  //region PendingOutboundTransactions (List)
  static pendingOutboundTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionsGetLength`);
    return result;
  }

  static pendingOutboundTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.pending_outbound_transactions_get_at(
      ptr,
      position,
      error
    );
    this.checkErrorResult(error, `pendingOutboundTransactionsGetAt`);
    return result;
  }

  static pendingOutboundTransactionsDestroy(ptr) {
    this.#fn.pending_outbound_transactions_destroy(ptr);
  }
  //endregion

  //region PendingInboundTransaction
  static pendingInboundTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_transaction_id(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionGetTransactionId`);
    return result;
  }

  static pendingInboundTransactionGetSourcePublicKey(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_source_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionGetSourcePublicKey`);
    return result;
  }

  static pendingInboundTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetMessage`);
    return result;
  }

  static pendingInboundTransactionGetAmount(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetAmount`);
    return result;
  }

  static pendingInboundTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_timestamp(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetTimestamp`);
    return result;
  }

  static pendingInboundTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetStatus`);
    return result;
  }

  static pendingInboundTransactionDestroy(ptr) {
    this.#fn.pending_inbound_transaction_destroy(ptr);
  }
  //endregion

  //region PendingInboundTransactions (List)
  static pendingInboundTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionsGetLength`);
    return result;
  }

  static pendingInboundTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.#fn.pending_inbound_transactions_get_at(
      ptr,
      position,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionsGetAt`);
    return result;
  }

  static pendingInboundTransactionsDestroy(ptr) {
    this.#fn.pending_inbound_transactions_destroy(ptr);
  }
  //endregion

  //region Wallet

  //region Callbacks
  static createCallbackReceivedTransaction(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }

  static createCallbackReceivedTransactionReply(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }

  static createCallbackReceivedFinalizedTransaction(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }

  static createCallbackTransactionBroadcast(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }

  static createCallbackTransactionMined(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }

  static createCallbackTransactionMinedUnconfirmed(fn) {
    return ffi.Callback("void", ["pointer", "uint64"], fn);
  }

  static createCallbackDirectSendResult(fn) {
    return ffi.Callback("void", ["uint64", "bool"], fn);
  }

  static createCallbackStoreAndForwardSendResult(fn) {
    return ffi.Callback("void", ["uint64", "bool"], fn);
  }

  static createCallbackTransactionCancellation(fn) {
    return ffi.Callback("void", ["pointer"], fn);
  }
  static createCallbackUtxoValidationComplete(fn) {
    return ffi.Callback("void", ["uint64", "uchar"], fn);
  }
  static createCallbackStxoValidationComplete(fn) {
    return ffi.Callback("void", ["uint64", "uchar"], fn);
  }
  static createCallbackInvalidTxoValidationComplete(fn) {
    return ffi.Callback("void", ["uint64", "uchar"], fn);
  }
  static createCallbackTransactionValidationComplete(fn) {
    return ffi.Callback("void", ["uint64", "uchar"], fn);
  }
  static createCallbackSafMessageReceived(fn) {
    return ffi.Callback("void", ["void"], fn);
  }
  static createRecoveryProgressCallback(fn) {
    return ffi.Callback("void", ["uchar", "uint64", "uint64"], fn);
  }
  //endregion

  static walletCreate(
    config,
    log_path,
    num_rolling_log_files,
    size_per_log_file_bytes,
    passphrase,
    seed_words,
    callback_received_transaction,
    callback_received_transaction_reply,
    callback_received_finalized_transaction,
    callback_transaction_broadcast,
    callback_transaction_mined,
    callback_transaction_mined_unconfirmed,
    callback_direct_send_result,
    callback_store_and_forward_send_result,
    callback_transaction_cancellation,
    callback_utxo_validation_complete,
    callback_stxo_validation_complete,
    callback_invalid_txo_validation_complete,
    callback_transaction_validation_complete,
    callback_saf_message_received
  ) {
    let error = this.initError();
    let recovery_in_progress = this.initBool();

    let result = this.#fn.wallet_create(
      config,
      log_path,
      num_rolling_log_files,
      size_per_log_file_bytes,
      passphrase,
      seed_words,
      callback_received_transaction,
      callback_received_transaction_reply,
      callback_received_finalized_transaction,
      callback_transaction_broadcast,
      callback_transaction_mined,
      callback_transaction_mined_unconfirmed,
      callback_direct_send_result,
      callback_store_and_forward_send_result,
      callback_transaction_cancellation,
      callback_utxo_validation_complete,
      callback_stxo_validation_complete,
      callback_invalid_txo_validation_complete,
      callback_transaction_validation_complete,
      callback_saf_message_received,
      recovery_in_progress,
      error
    );
    this.checkErrorResult(error, `walletCreate`);
    if (recovery_in_progress) {
      console.log("Wallet recovery is in progress");
    }
    return result;
  }

  static walletGetPublicKey(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_public_key(ptr, error);
    this.checkErrorResult(error, `walletGetPublicKey`);
    return result;
  }

  static walletSignMessage(ptr, msg) {
    let error = this.initError();
    let result = this.#fn.wallet_sign_message(ptr, msg, error);
    this.checkErrorResult(error, `walletSignMessage`);
    return result;
  }

  static walletVerifyMessageSignature(ptr, public_key_ptr, hex_sig_nonce, msg) {
    let error = this.initError();
    let result = this.#fn.wallet_verify_message_signature(
      ptr,
      public_key_ptr,
      hex_sig_nonce,
      msg,
      error
    );
    this.checkErrorResult(error, `walletVerifyMessageSignature`);
    return result;
  }

  static walletAddBaseNodePeer(ptr, public_key_ptr, address) {
    let error = this.initError();
    let result = this.#fn.wallet_add_base_node_peer(
      ptr,
      public_key_ptr,
      address,
      error
    );
    this.checkErrorResult(error, `walletAddBaseNodePeer`);
    return result;
  }

  static walletUpsertContact(ptr, contact_ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_upsert_contact(ptr, contact_ptr, error);
    this.checkErrorResult(error, `walletUpsertContact`);
    return result;
  }

  static walletRemoveContact(ptr, contact_ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_remove_contact(ptr, contact_ptr, error);
    this.checkErrorResult(error, `walletRemoveContact`);
    return result;
  }

  static walletGetAvailableBalance(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_available_balance(ptr, error);
    this.checkErrorResult(error, `walletGetAvailableBalance`);
    return result;
  }

  static walletGetPendingIncomingBalance(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_incoming_balance(ptr, error);
    this.checkErrorResult(error, `walletGetPendingIncomingBalance`);
    return result;
  }

  static walletGetPendingOutgoingBalance(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_outgoing_balance(ptr, error);
    this.checkErrorResult(error, `walletGetPendingOutgoingBalance`);
    return result;
  }

  static walletGetFeeEstimate(
    ptr,
    amount,
    fee_per_gram,
    num_kernels,
    num_outputs
  ) {
    let error = this.initError();
    let result = this.#fn.wallet_get_fee_estimate(
      ptr,
      amount,
      fee_per_gram,
      num_kernels,
      num_outputs,
      error
    );
    this.checkErrorResult(error, `walletGetFeeEstimate`);
    return result;
  }

  static walletGetNumConfirmationsRequired(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_num_confirmations_required(ptr, error);
    this.checkErrorResult(error, `walletGetNumConfirmationsRequired`);
    return result;
  }

  static walletSetNumConfirmationsRequired(ptr, num) {
    let error = this.initError();
    this.#fn.wallet_set_num_confirmations_required(ptr, num, error);
    this.checkErrorResult(error, `walletSetNumConfirmationsRequired`);
  }

  static walletSendTransaction(
    ptr,
    destination,
    amount,
    fee_per_gram,
    message
  ) {
    let error = this.initError();
    let result = this.#fn.wallet_send_transaction(
      ptr,
      destination,
      amount,
      fee_per_gram,
      message,
      error
    );
    this.checkErrorResult(error, `walletSendTransaction`);
    return result;
  }

  static walletGetContacts(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_contacts(ptr, error);
    this.checkErrorResult(error, `walletGetContacts`);
    return result;
  }

  static walletGetCompletedTransactions(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_completed_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetCompletedTransactions`);
    return result;
  }

  static walletGetPendingOutboundTransactions(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_outbound_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetPendingOutboundTransactions`);
    return result;
  }

  static walletGetPendingInboundTransactions(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_inbound_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetPendingInboundTransactions`);
    return result;
  }

  static walletGetCancelledTransactions(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_cancelled_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetCancelledTransactions`);
    return result;
  }

  static walletGetCompletedTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.#fn.wallet_get_completed_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetCompletedTransactionById`);
    return result;
  }

  static walletGetPendingOutboundTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_outbound_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetPendingOutboundTransactionById`);
    return result;
  }

  static walletGetPendingInboundTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.#fn.wallet_get_pending_inbound_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetPendingInboundTransactionById`);
    return result;
  }

  static walletGetCancelledTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.#fn.wallet_get_cancelled_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetCancelledTransactionById`);
    return result;
  }

  static walletImportUtxo(
    ptr,
    amount,
    spending_key_ptr,
    source_public_key_ptr,
    message
  ) {
    let error = this.initError();
    let result = this.#fn.wallet_import_utxo(
      ptr,
      amount,
      spending_key_ptr,
      source_public_key_ptr,
      message,
      error
    );
    this.checkErrorResult(error, `walletImportUtxo`);
    return result;
  }

  static walletStartUtxoValidation(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_start_utxo_validation(ptr, error);
    this.checkErrorResult(error, `walletStartUtxoValidation`);
    return result;
  }

  static walletStartStxoValidation(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_start_stxo_validation(ptr, error);
    this.checkErrorResult(error, `walletStartStxoValidation`);
    return result;
  }

  static walletStartInvalidTxoValidation(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_start_invalid_txo_validation(ptr, error);
    this.checkErrorResult(error, `walletStartInvalidUtxoValidation`);
    return result;
  }

  static walletStartTransactionValidation(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_start_transaction_validation(ptr, error);
    this.checkErrorResult(error, `walletStartTransactionValidation`);
    return result;
  }

  static walletRestartTransactionBroadcast(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_restart_transaction_broadcast(ptr, error);
    this.checkErrorResult(error, `walletRestartTransactionBroadcast`);
    return result;
  }

  static walletSetLowPowerMode(ptr) {
    let error = this.initError();
    this.#fn.wallet_set_low_power_mode(ptr, error);
    this.checkErrorResult(error, `walletSetLowPowerMode`);
  }

  static walletSetNormalPowerMode(ptr) {
    let error = this.initError();
    this.#fn.wallet_set_normal_power_mode(ptr, error);
    this.checkErrorResult(error, `walletSetNormalPowerMode`);
  }

  static walletCancelPendingTransaction(ptr, transaction_id) {
    let error = this.initError();
    let result = this.#fn.wallet_cancel_pending_transaction(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletCancelPendingTransaction`);
    return result;
  }

  static walletCoinSplit(ptr, amount, count, fee, msg, lock_height) {
    let error = this.initError();
    let result = this.#fn.wallet_coin_split(
      ptr,
      amount,
      count,
      fee,
      msg,
      lock_height,
      error
    );
    this.checkErrorResult(error, `walletCoinSplit`);
    return result;
  }

  static walletGetSeedWords(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_seed_words(ptr, error);
    this.checkErrorResult(error, `walletGetSeedWords`);
    return result;
  }

  static walletApplyEncryption(ptr, passphrase) {
    let error = this.initError();
    this.#fn.wallet_apply_encryption(ptr, passphrase, error);
    this.checkErrorResult(error, `walletApplyEncryption`);
  }

  static walletRemoveEncryption(ptr) {
    let error = this.initError();
    this.#fn.wallet_remove_encryption(ptr, error);
    this.checkErrorResult(error, `walletRemoveEncryption`);
  }

  static walletSetKeyValue(ptr, key_ptr, value) {
    let error = this.initError();
    let result = this.#fn.wallet_set_key_value(ptr, key_ptr, value, error);
    this.checkErrorResult(error, `walletSetKeyValue`);
    return result;
  }

  static walletGetValue(ptr, key_ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_get_value(ptr, key_ptr, error);
    this.checkErrorResult(error, `walletGetValue`);
    return result;
  }

  static walletClearValue(ptr, key_ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_clear_value(ptr, key_ptr, error);
    this.checkErrorResult(error, `walletClearValue`);
    return result;
  }

  static walletIsRecoveryInProgress(ptr) {
    let error = this.initError();
    let result = this.#fn.wallet_is_recovery_in_progress(ptr, error);
    this.checkErrorResult(error, `walletIsRecoveryInProgress`);
    return result;
  }

  static walletStartRecovery(
    ptr,
    base_node_public_key_ptr,
    recovery_progress_callback
  ) {
    let error = this.initError();
    let result = this.#fn.wallet_start_recovery(
      ptr,
      base_node_public_key_ptr,
      recovery_progress_callback,
      error
    );
    this.checkErrorResult(error, `walletStartRecovery`);
    return result;
  }

  static walletDestroy(ptr) {
    this.#fn.wallet_destroy(ptr);
  }
  //endregion
}
module.exports = InterfaceFFI;
