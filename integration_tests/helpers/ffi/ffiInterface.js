// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

/**
 * NB!: Modify with caution.
 **/

const { expect } = require("chai");
const ffi = require("ffi-napi");
const ref = require("ref-napi");
const dateFormat = require("dateformat");
const { spawn } = require("child_process");
const fs = require("fs");

class InterfaceFFI {
  static void = ref.types.void;
  static bool = ref.types.bool;
  static int = ref.types.int;
  static ulonglong = ref.types.uint64; // Note: 'ref.types.ulonglong' has a memory alignment problem
  static uchar = ref.types.uchar;
  static uint = ref.types.uint;
  static string = ref.types.CString;
  static ucharPtr = ref.refType(this.uchar); // uchar*
  static ptr = ref.refType(this.void); //pointer is opaque
  static stringPtr = ref.refType(this.string);
  static intPtr = ref.refType(this.int); // int*
  static boolPtr = ref.refType(this.bool); // bool*
  static ushort = ref.types.ushort;

  //region Compile
  static compile() {
    return new Promise((resolve, _reject) => {
      const cmd = "cargo";
      const args = [
        "build",
        "--release",
        "--locked",
        "--package",
        "tari_wallet_ffi",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out/ffi",
      ];
      const baseDir = `./temp/base_nodes/${dateFormat(
        new Date(),
        "yyyymmddHHMM"
      )}/WalletFFI-compile`;
      if (!fs.existsSync(baseDir)) {
        fs.mkdirSync(baseDir, { recursive: true });
        fs.mkdirSync(baseDir + "/log", { recursive: true });
      }
      const ps = spawn(cmd, args);
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
      this.ps = ps;
    });
  }
  //endregion

  //region Interface
  static fn;

  static loaded = false;
  static ps = null;
  static library = null;

  static async init() {
    let platform = process.platform === "win32" ? "" : "lib";
    this.library = `${process.cwd()}/temp/out/ffi/${platform}tari_wallet_ffi`;
    // Load the library
    this.fn = ffi.Library(this.loaded ? null : this.library, {
      transport_memory_create: [this.ptr, []],
      transport_tcp_create: [this.ptr, [this.string, this.intPtr]],
      transport_tor_create: [
        this.ptr,
        [
          this.string,
          this.ptr,
          this.ushort,
          this.bool,
          this.string,
          this.string,
          this.intPtr,
        ],
      ],
      transport_memory_get_address: [this.stringPtr, [this.ptr, this.intPtr]],
      transport_config_destroy: [this.void, [this.ptr]],
      string_destroy: [this.void, [this.string]],
      byte_vector_create: [this.ptr, [this.ucharPtr, this.uint, this.intPtr]],
      byte_vector_get_at: [this.uchar, [this.ptr, this.uint, this.intPtr]],
      byte_vector_get_length: [this.uint, [this.ptr, this.intPtr]],
      byte_vector_destroy: [this.void, [this.ptr]],
      public_key_create: [this.ptr, [this.ptr, this.intPtr]],
      public_key_get_bytes: [this.ptr, [this.ptr, this.intPtr]],
      public_key_from_private_key: [this.ptr, [this.ptr, this.intPtr]],
      public_key_from_hex: [this.ptr, [this.string, this.intPtr]],
      public_key_destroy: [this.void, [this.ptr]],
      public_key_to_emoji_id: [this.stringPtr, [this.ptr, this.intPtr]],
      emoji_id_to_public_key: [this.ptr, [this.string, this.intPtr]],
      private_key_create: [this.ptr, [this.ptr, this.intPtr]],
      private_key_generate: [this.ptr, []],
      private_key_get_bytes: [this.ptr, [this.ptr, this.intPtr]],
      private_key_from_hex: [this.ptr, [this.string, this.intPtr]],
      private_key_destroy: [this.void, [this.ptr]],
      seed_words_create: [this.ptr, []],
      seed_words_get_mnemonic_word_list_for_language: [
        this.ptr,
        [this.string, this.intPtr],
      ],
      seed_words_get_length: [this.uint, [this.ptr, this.intPtr]],
      seed_words_get_at: [this.stringPtr, [this.ptr, this.uint, this.intPtr]],
      seed_words_push_word: [this.uchar, [this.ptr, this.string, this.intPtr]],
      seed_words_destroy: [this.void, [this.ptr]],
      contact_create: [this.ptr, [this.string, this.ptr, this.intPtr]],
      contact_get_alias: [this.stringPtr, [this.ptr, this.intPtr]],
      contact_get_public_key: [this.ptr, [this.ptr, this.intPtr]],
      contact_destroy: [this.void, [this.ptr]],
      contacts_get_length: [this.uint, [this.ptr, this.intPtr]],
      contacts_get_at: [this.ptr, [this.ptr, this.uint, this.intPtr]],
      contacts_destroy: [this.void, [this.ptr]],
      completed_transaction_get_destination_public_key: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_source_public_key: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_amount: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_fee: [this.ulonglong, [this.ptr, this.intPtr]],
      completed_transaction_get_message: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_status: [this.int, [this.ptr, this.intPtr]],
      completed_transaction_get_transaction_id: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_timestamp: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_is_outbound: [this.bool, [this.ptr, this.intPtr]],
      completed_transaction_get_confirmations: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_destroy: [this.void, [this.ptr]],
      completed_transaction_get_transaction_kernel: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      completed_transaction_get_cancellation_reason: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      completed_transactions_get_length: [this.uint, [this.ptr, this.intPtr]],
      completed_transactions_get_at: [
        this.ptr,
        [this.ptr, this.uint, this.intPtr],
      ],
      completed_transactions_destroy: [this.void, [this.ptr]],
      transaction_kernel_get_excess_hex: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      transaction_kernel_get_excess_public_nonce_hex: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      transaction_kernel_get_excess_signature_hex: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      transaction_kernel_destroy: [this.void, [this.ptr]],
      pending_outbound_transaction_get_transaction_id: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_destination_public_key: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_amount: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_fee: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_message: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_timestamp: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_get_status: [
        this.int,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transaction_destroy: [this.void, [this.ptr]],
      pending_outbound_transactions_get_length: [
        this.uint,
        [this.ptr, this.intPtr],
      ],
      pending_outbound_transactions_get_at: [
        this.ptr,
        [this.ptr, this.uint, this.intPtr],
      ],
      pending_outbound_transactions_destroy: [this.void, [this.ptr]],
      pending_inbound_transaction_get_transaction_id: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_get_source_public_key: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_get_message: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_get_amount: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_get_timestamp: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_get_status: [
        this.int,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transaction_destroy: [this.void, [this.ptr]],
      pending_inbound_transactions_get_length: [
        this.uint,
        [this.ptr, this.intPtr],
      ],
      pending_inbound_transactions_get_at: [
        this.ptr,
        [this.ptr, this.uint, this.intPtr],
      ],
      pending_inbound_transactions_destroy: [this.void, [this.ptr]],
      transaction_send_status_decode: [this.uint, [this.ptr, this.intPtr]],
      transaction_send_status_destroy: [this.void, [this.ptr]],
      comms_config_create: [
        this.ptr,
        [
          this.string,
          this.ptr,
          this.string,
          this.string,
          this.ulonglong,
          this.ulonglong,
          this.intPtr,
        ],
      ],
      comms_config_destroy: [this.void, [this.ptr]],
      comms_list_connected_public_keys: [this.ptr, [this.ptr, this.intPtr]],
      wallet_create: [
        this.ptr,
        [
          this.ptr,
          this.string,
          this.uint,
          this.uint,
          this.string,
          this.ptr,
          this.string,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.boolPtr,
          this.intPtr,
        ],
      ],
      wallet_get_balance: [this.ptr, [this.ptr, this.intPtr]],
      wallet_sign_message: [
        this.stringPtr,
        [this.ptr, this.string, this.intPtr],
      ],
      wallet_verify_message_signature: [
        this.bool,
        [this.ptr, this.ptr, this.string, this.string, this.intPtr],
      ],
      wallet_add_base_node_peer: [
        this.bool,
        [this.ptr, this.ptr, this.string, this.intPtr],
      ],
      wallet_upsert_contact: [this.bool, [this.ptr, this.ptr, this.intPtr]],
      wallet_remove_contact: [this.bool, [this.ptr, this.ptr, this.intPtr]],
      balance_get_available: [this.ulonglong, [this.ptr, this.intPtr]],
      balance_get_time_locked: [this.ulonglong, [this.ptr, this.intPtr]],
      balance_get_pending_incoming: [this.ulonglong, [this.ptr, this.intPtr]],
      balance_get_pending_outgoing: [this.ulonglong, [this.ptr, this.intPtr]],
      liveness_data_get_public_key: [this.ptr, [this.ptr, this.intPtr]],
      liveness_data_get_latency: [this.int, [this.ptr, this.intPtr]],
      liveness_data_get_last_seen: [this.stringPtr, [this.ptr, this.intPtr]],
      liveness_data_get_message_type: [this.int, [this.ptr, this.intPtr]],
      liveness_data_get_online_status: [
        this.stringPtr,
        [this.ptr, this.intPtr],
      ],
      wallet_get_fee_estimate: [
        this.ulonglong,
        [
          this.ptr,
          this.ulonglong,
          this.ulonglong,
          this.ulonglong,
          this.ulonglong,
          this.intPtr,
        ],
      ],
      wallet_get_num_confirmations_required: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      wallet_set_num_confirmations_required: [
        this.void,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_send_transaction: [
        this.ulonglong,
        [
          this.ptr,
          this.ptr,
          this.ulonglong,
          this.ulonglong,
          this.string,
          this.bool,
          this.intPtr,
        ],
      ],
      wallet_get_contacts: [this.ptr, [this.ptr, this.intPtr]],
      wallet_get_completed_transactions: [this.ptr, [this.ptr, this.intPtr]],
      wallet_get_pending_outbound_transactions: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      wallet_get_public_key: [this.ptr, [this.ptr, this.intPtr]],
      wallet_get_pending_inbound_transactions: [
        this.ptr,
        [this.ptr, this.intPtr],
      ],
      wallet_get_cancelled_transactions: [this.ptr, [this.ptr, this.intPtr]],
      wallet_get_completed_transaction_by_id: [
        this.ptr,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_get_pending_outbound_transaction_by_id: [
        this.ptr,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_get_pending_inbound_transaction_by_id: [
        this.ptr,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_get_cancelled_transaction_by_id: [
        this.ptr,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_import_external_utxo_as_non_rewindable: [
        this.ulonglong,
        [
          this.ptr,
          this.ulonglong,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ptr,
          this.ulonglong,
          this.string,
          this.intPtr,
        ],
      ],
      wallet_start_txo_validation: [this.ulonglong, [this.ptr, this.intPtr]],
      wallet_start_transaction_validation: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      wallet_restart_transaction_broadcast: [
        this.bool,
        [this.ptr, this.intPtr],
      ],
      wallet_set_low_power_mode: [this.void, [this.ptr, this.intPtr]],
      wallet_set_normal_power_mode: [this.void, [this.ptr, this.intPtr]],
      wallet_cancel_pending_transaction: [
        this.bool,
        [this.ptr, this.ulonglong, this.intPtr],
      ],
      wallet_coin_split: [
        this.ulonglong,
        [
          this.ptr,
          this.ulonglong,
          this.ulonglong,
          this.ulonglong,
          this.string,
          this.ulonglong,
          this.intPtr,
        ],
      ],
      wallet_get_seed_words: [this.ptr, [this.ptr, this.intPtr]],
      wallet_apply_encryption: [
        this.void,
        [this.ptr, this.string, this.intPtr],
      ],
      wallet_remove_encryption: [this.void, [this.ptr, this.intPtr]],
      wallet_set_key_value: [
        this.bool,
        [this.ptr, this.string, this.string, this.intPtr],
      ],
      wallet_get_value: [this.stringPtr, [this.ptr, this.string, this.intPtr]],
      wallet_clear_value: [this.bool, [this.ptr, this.string, this.intPtr]],
      wallet_is_recovery_in_progress: [this.bool, [this.ptr, this.intPtr]],
      wallet_start_recovery: [
        this.bool,
        [this.ptr, this.ptr, this.ptr, this.intPtr],
      ],
      wallet_destroy: [this.void, [this.ptr]],
      balance_destroy: [this.void, [this.ptr]],
      liveness_data_destroy: [this.void, [this.ptr]],
      file_partial_backup: [this.void, [this.string, this.string, this.intPtr]],
      log_debug_message: [this.void, [this.string]],
      get_emoji_set: [this.ptr, []],
      emoji_set_destroy: [this.void, [this.ptr]],
      emoji_set_get_at: [this.ptr, [this.ptr, this.uint, this.intPtr]],
      emoji_set_get_length: [this.uint, [this.ptr, this.intPtr]],
      wallet_get_fee_per_gram_stats: [
        this.ptr,
        [this.ptr, this.uint, this.intPtr],
      ],
      fee_per_gram_stats_get_length: [this.uint, [this.ptr, this.intPtr]],
      fee_per_gram_stats_get_at: [this.ptr, [this.ptr, this.uint, this.intPtr]],
      fee_per_gram_stats_destroy: [this.void, [this.ptr]],
      fee_per_gram_stat_get_order: [this.ulonglong, [this.ptr, this.intPtr]],
      fee_per_gram_stat_get_min_fee_per_gram: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      fee_per_gram_stat_get_avg_fee_per_gram: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      fee_per_gram_stat_get_max_fee_per_gram: [
        this.ulonglong,
        [this.ptr, this.intPtr],
      ],
      fee_per_gram_stat_destroy: [this.void, [this.ptr]],
    });

    this.loaded = true;
  }
  //endregion

  static checkErrorResult(error, error_name) {
    expect(error.deref()).to.equal(0, `Error in ${error_name}`);
  }

  //region Helpers
  static initError() {
    let error = ref.alloc(ref.types.int);
    return error;
  }

  static initBool() {
    let boolean = ref.alloc(ref.types.bool);
    return boolean;
  }

  static filePartialBackup(original_file_path, backup_file_path) {
    let error = this.initError();
    let result = this.fn.file_partial_backup(
      original_file_path,
      backup_file_path,
      error
    );
    this.checkErrorResult(error, `filePartialBackup`);
    return result;
  }

  static logDebugMessage(msg) {
    this.fn.log_debug_message(msg);
  }
  //endregion

  //region String
  static stringDestroy(s) {
    this.fn.string_destroy(s);
  }
  //endregion

  // region ByteVector
  static byteVectorCreate(byte_array, element_count) {
    let error = this.initError();
    let result = this.fn.byte_vector_create(byte_array, element_count, error);
    this.checkErrorResult(error, `byteVectorCreate`);
    return result;
  }

  static byteVectorGetAt(ptr, i) {
    let error = this.initError();
    let result = this.fn.byte_vector_get_at(ptr, i, error);
    this.checkErrorResult(error, `byteVectorGetAt`);
    return result;
  }

  static byteVectorGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.byte_vector_get_length(ptr, error);
    this.checkErrorResult(error, `byteVectorGetLength`);
    return result;
  }

  static byteVectorDestroy(ptr) {
    this.fn.byte_vector_destroy(ptr);
  }
  //endregion

  //region PrivateKey
  static privateKeyCreate(ptr) {
    let error = this.initError();
    let result = this.fn.private_key_create(ptr, error);
    this.checkErrorResult(error, `privateKeyCreate`);
    return result;
  }

  static privateKeyGenerate() {
    return this.fn.private_key_generate();
  }

  static privateKeyGetBytes(ptr) {
    let error = this.initError();
    let result = this.fn.private_key_get_bytes(ptr, error);
    this.checkErrorResult(error, "privateKeyGetBytes");
    return result;
  }

  static privateKeyFromHex(hex) {
    let error = this.initError();
    let result = this.fn.private_key_from_hex(hex, error);
    this.checkErrorResult(error, "privateKeyFromHex");
    return result;
  }

  static privateKeyDestroy(ptr) {
    this.fn.private_key_destroy(ptr);
  }

  //endregion

  //region PublicKey
  static publicKeyCreate(ptr) {
    let error = this.initError();
    let result = this.fn.public_key_create(ptr, error);
    this.checkErrorResult(error, `publicKeyCreate`);
    return result;
  }

  static publicKeyGetBytes(ptr) {
    let error = this.initError();
    let result = this.fn.public_key_get_bytes(ptr, error);
    this.checkErrorResult(error, `publicKeyGetBytes`);
    return result;
  }

  static publicKeyFromPrivateKey(ptr) {
    let error = this.initError();
    let result = this.fn.public_key_from_private_key(ptr, error);
    this.checkErrorResult(error, `publicKeyFromPrivateKey`);
    return result;
  }

  static publicKeyFromHex(hex) {
    let error = this.initError();
    let result = this.fn.public_key_from_hex(hex, error);
    this.checkErrorResult(error, `publicKeyFromHex`);
    return result;
  }

  static emojiIdToPublicKey(emoji) {
    let error = this.initError();
    let result = this.fn.emoji_id_to_public_key(emoji, error);
    this.checkErrorResult(error, `emojiIdToPublicKey`);
    return result;
  }

  static publicKeyToEmojiId(ptr) {
    let error = this.initError();
    let result = this.fn.public_key_to_emoji_id(ptr, error);
    this.checkErrorResult(error, `publicKeyToEmojiId`);
    return result;
  }

  static publicKeyDestroy(ptr) {
    this.fn.public_key_destroy(ptr);
  }
  //endregion

  //region TransportType
  static transportMemoryCreate() {
    return this.fn.transport_memory_create();
  }

  static transportTcpCreate(listener_address) {
    let error = this.initError();
    let result = this.fn.transport_tcp_create(listener_address, error);
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
    let result = this.fn.transport_tor_create(
      control_server_address,
      tor_cookie,
      tor_port,
      true,
      socks_username,
      socks_password,
      error
    );
    this.checkErrorResult(error, `transportTorCreate`);
    return result;
  }

  static transportMemoryGetAddress(transport) {
    let error = this.initError();
    let result = this.fn.transport_memory_get_address(transport, error);
    this.checkErrorResult(error, `transportMemoryGetAddress`);
    return result;
  }

  static transportConfigDestroy(transport) {
    this.fn.transport_config_destroy(transport);
  }
  //endregion

  //region EmojiSet
  static getEmojiSet() {
    return this.fn.this.fn.get_emoji_set();
  }

  static emojiSetDestroy(ptr) {
    this.fn.emoji_set_destroy(ptr);
  }

  static emojiSetGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.emoji_set_get_at(ptr, position, error);
    this.checkErrorResult(error, `emojiSetGetAt`);
    return result;
  }

  static emojiSetGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.emoji_set_get_length(ptr, error);
    this.checkErrorResult(error, `emojiSetGetLength`);
    return result;
  }
  //endregion

  //region SeedWords
  static seedWordsCreate() {
    return this.fn.seed_words_create();
  }

  static seedWordsGetMnemonicWordListForLanguage(language) {
    let error = this.initError();
    let result = this.fn.seed_words_get_mnemonic_word_list_for_language(
      language,
      error
    );
    this.checkErrorResult(error, `seedWordsGetMnemonicWordListForLanguage`);
    return result;
  }

  static seedWordsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.seed_words_get_length(ptr, error);
    this.checkErrorResult(error, `emojiSetGetLength`);
    return result;
  }

  static seedWordsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.seed_words_get_at(ptr, position, error);
    this.checkErrorResult(error, `seedWordsGetAt`);
    return result;
  }

  static seedWordsPushWord(ptr, word) {
    let error = this.initError();
    let result = this.fn.seed_words_push_word(ptr, word, error);
    this.checkErrorResult(error, `seedWordsPushWord`);
    return result;
  }

  static seedWordsDestroy(ptr) {
    this.fn.seed_words_destroy(ptr);
  }
  //endregion

  //region CommsConfig
  static commsConfigCreate(
    public_address,
    transport,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs
  ) {
    let error = this.initError();
    let result = this.fn.comms_config_create(
      public_address,
      transport,
      database_name,
      datastore_path,
      discovery_timeout_in_secs,
      saf_message_duration_in_secs,
      error
    );
    this.checkErrorResult(error, `commsConfigCreate`);
    return result;
  }

  static commsConfigDestroy(ptr) {
    this.fn.comms_config_destroy(ptr);
  }

  static commsListConnectedPublicKeys(walletPtr) {
    let error = this.initError();
    let result = this.fn.comms_list_connected_public_keys(walletPtr, error);
    this.checkErrorResult(error, `commsListConnectedPublicKeys`);
    return result;
  }
  //endregion

  //region Contact
  static contactCreate(alias, public_key) {
    let error = this.initError();
    let result = this.fn.contact_create(alias, public_key, error);
    this.checkErrorResult(error, `contactCreate`);
    return result;
  }

  static contactGetAlias(ptr) {
    let error = this.initError();
    let result = this.fn.contact_get_alias(ptr, error);
    this.checkErrorResult(error, `contactGetAlias`);
    return result;
  }

  static contactGetPublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.contact_get_public_key(ptr, error);
    this.checkErrorResult(error, `contactGetPublicKey`);
    return result;
  }

  static contactDestroy(ptr) {
    this.fn.contact_destroy(ptr);
  }
  //endregion

  //region Contacts (List)
  static contactsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.contacts_get_length(ptr, error);
    this.checkErrorResult(error, `contactsGetLength`);
    return result;
  }

  static contactsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.contacts_get_at(ptr, position, error);
    this.checkErrorResult(error, `contactsGetAt`);
    return result;
  }

  static contactsDestroy(ptr) {
    this.fn.contacts_destroy(ptr);
  }
  //endregion

  //region CompletedTransaction
  static completedTransactionGetDestinationPublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_destination_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetDestinationPublicKey`);
    return result;
  }

  static completedTransactionGetSourcePublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_source_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetSourcePublicKey`);
    return result;
  }

  static completedTransactionGetAmount(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetAmount`);
    return result;
  }

  static completedTransactionGetFee(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_fee(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetFee`);
    return result;
  }

  static completedTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetMessage`);
    return result;
  }

  static completedTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetStatus`);
    return result;
  }

  static completedTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_transaction_id(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetTransactionId`);
    return result;
  }

  static completedTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_timestamp(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetTimestamp`);
    return result;
  }

  static completedTransactionIsOutbound(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_is_outbound(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetIsOutbound`);
    return result;
  }

  static completedTransactionGetConfirmations(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_confirmations(ptr, error);
    this.checkErrorResult(error, `completedTransactionGetConfirmations`);
    return result;
  }

  static completedTransactionGetKernel(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_transaction_kernel(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetKernel`);
    return result;
  }

  static completedTransactionGetCancellationReason(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transaction_get_cancellation_reason(
      ptr,
      error
    );
    this.checkErrorResult(error, `completedTransactionGetCancellationReason`);
    return result;
  }

  static completedTransactionDestroy(ptr) {
    this.fn.completed_transaction_destroy(ptr);
  }

  //endregion

  //region TransactionKernel
  static transactionKernelGetExcess(ptr) {
    let error = this.initError();
    let result = this.fn.transaction_kernel_get_excess_hex(ptr, error);
    this.checkErrorResult(error, `transactionKernelGetExcess`);
    return result;
  }

  static transactionKernelGetExcessPublicNonce(ptr) {
    let error = this.initError();
    let result = this.fn.transaction_kernel_get_excess_public_nonce_hex(
      ptr,
      error
    );
    this.checkErrorResult(error, `transactionKernelGetExcessPublicNonce`);
    return result;
  }

  static transactionKernelGetExcessSigntature(ptr) {
    let error = this.initError();
    let result = this.fn.transaction_kernel_get_excess_signature_hex(
      ptr,
      error
    );
    this.checkErrorResult(error, `transactionKernelGetExcessSigntature`);
    return result;
  }

  static transactionKernelDestroy(ptr) {
    this.fn.transaction_kernel_destroy(ptr);
  }
  //endRegion

  //region CompletedTransactions (List)
  static completedTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.completed_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `completedTransactionsGetLength`);
    return result;
  }

  static completedTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.completed_transactions_get_at(ptr, position, error);
    this.checkErrorResult(error, `completedTransactionsGetAt`);
    return result;
  }

  static completedTransactionsDestroy(transactions) {
    this.fn.completed_transactions_destroy(transactions);
  }
  //endregion

  //region PendingOutboundTransaction
  static pendingOutboundTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transaction_get_transaction_id(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingOutboundTransactionGetTransactionId`);
    return result;
  }

  static pendingOutboundTransactionGetDestinationPublicKey(ptr) {
    let error = this.initError();
    let result =
      this.fn.pending_outbound_transaction_get_destination_public_key(
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
    let result = this.fn.pending_outbound_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetAmount`);
    return result;
  }

  static pendingOutboundTransactionGetFee(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transaction_get_fee(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetFee`);
    return result;
  }

  static pendingOutboundTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetMessage`);
    return result;
  }

  static pendingOutboundTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transaction_get_timestamp(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetTimestamp`);
    return result;
  }

  static pendingOutboundTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionGetStatus`);
    return result;
  }

  static pendingOutboundTransactionDestroy(ptr) {
    this.fn.pending_outbound_transaction_destroy(ptr);
  }
  //endregion

  //region PendingOutboundTransactions (List)
  static pendingOutboundTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `pendingOutboundTransactionsGetLength`);
    return result;
  }

  static pendingOutboundTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.pending_outbound_transactions_get_at(
      ptr,
      position,
      error
    );
    this.checkErrorResult(error, `pendingOutboundTransactionsGetAt`);
    return result;
  }

  static pendingOutboundTransactionsDestroy(ptr) {
    this.fn.pending_outbound_transactions_destroy(ptr);
  }
  //endregion

  //region PendingInboundTransaction
  static pendingInboundTransactionGetTransactionId(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_transaction_id(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionGetTransactionId`);
    return result;
  }

  static pendingInboundTransactionGetSourcePublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_source_public_key(
      ptr,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionGetSourcePublicKey`);
    return result;
  }

  static pendingInboundTransactionGetMessage(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_message(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetMessage`);
    return result;
  }

  static pendingInboundTransactionGetAmount(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_amount(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetAmount`);
    return result;
  }

  static pendingInboundTransactionGetTimestamp(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_timestamp(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetTimestamp`);
    return result;
  }

  static pendingInboundTransactionGetStatus(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transaction_get_status(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionGetStatus`);
    return result;
  }

  static pendingInboundTransactionDestroy(ptr) {
    this.fn.pending_inbound_transaction_destroy(ptr);
  }
  //endregion

  //region PendingInboundTransactions (List)
  static pendingInboundTransactionsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transactions_get_length(ptr, error);
    this.checkErrorResult(error, `pendingInboundTransactionsGetLength`);
    return result;
  }

  static pendingInboundTransactionsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.pending_inbound_transactions_get_at(
      ptr,
      position,
      error
    );
    this.checkErrorResult(error, `pendingInboundTransactionsGetAt`);
    return result;
  }

  static pendingInboundTransactionsDestroy(ptr) {
    this.fn.pending_inbound_transactions_destroy(ptr);
  }
  //endregion

  //region TransactionSendStatus
  static transactionSendStatusDecode(ptr) {
    let error = this.initError();
    let result = this.fn.transaction_send_status_decode(ptr, error);
    this.checkErrorResult(error, `transactionSendStatusDecode`);
    return result;
  }

  static transactionSendStatusDestroy(ptr) {
    this.fn.transaction_send_status_destroy(ptr);
  }
  //endregion

  //region Callbacks
  static createCallbackReceivedTransaction(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackReceivedTransactionReply(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackReceivedFinalizedTransaction(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackTransactionBroadcast(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackTransactionMined(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackTransactionMinedUnconfirmed(fn) {
    return ffi.Callback(this.void, [this.ptr, this.ulonglong], fn);
  }

  static createCallbackFauxTransactionConfirmed(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }

  static createCallbackFauxTransactionUnconfirmed(fn) {
    return ffi.Callback(this.void, [this.ptr, this.ulonglong], fn);
  }

  static createCallbackTransactionSendResult(fn) {
    return ffi.Callback(this.void, [this.ulonglong, this.ptr], fn);
  }

  static createCallbackTransactionCancellation(fn) {
    return ffi.Callback(this.void, [this.ptr, this.ulonglong], fn);
  }
  static createCallbackTxoValidationComplete(fn) {
    return ffi.Callback(this.void, [this.ulonglong, this.uchar], fn);
  }
  static createCallbackContactsLivenessUpdated(fn) {
    return ffi.Callback(this.stringPtr, [this.ptr, this.intPtr], fn);
  }
  static createCallbackBalanceUpdated(fn) {
    return ffi.Callback(this.void, [this.ptr], fn);
  }
  static createCallbackTransactionValidationComplete(fn) {
    return ffi.Callback(this.void, [this.ulonglong, this.uchar], fn);
  }
  static createCallbackSafMessageReceived(fn) {
    return ffi.Callback(this.void, [], fn);
  }
  static createRecoveryProgressCallback(fn) {
    return ffi.Callback(
      this.void,
      [this.uchar, this.ulonglong, this.ulonglong],
      fn
    );
  }
  static createCallbackConnectivityStatus(fn) {
    return ffi.Callback(this.void, [this.ulonglong], fn);
  }
  //endregion

  static walletCreate(
    config,
    log_path,
    num_rolling_log_files,
    size_per_log_file_bytes,
    passphrase,
    seed_words,
    network,
    callback_received_transaction,
    callback_received_transaction_reply,
    callback_received_finalized_transaction,
    callback_transaction_broadcast,
    callback_transaction_mined,
    callback_transaction_mined_unconfirmed,
    callback_faux_transaction_confirmed,
    callback_faux_transaction_unconfirmed,
    callback_transaction_send_result,
    callback_transaction_cancellation,
    callback_txo_validation_complete,
    callback_contacts_liveness_data_updated,
    callback_balance_updated,
    callback_transaction_validation_complete,
    callback_saf_message_received,
    callback_connectivity_status
  ) {
    let error = this.initError();
    let recovery_in_progress = this.initBool();

    let result = this.fn.wallet_create(
      config,
      log_path,
      num_rolling_log_files,
      size_per_log_file_bytes,
      passphrase,
      seed_words,
      network,
      callback_received_transaction,
      callback_received_transaction_reply,
      callback_received_finalized_transaction,
      callback_transaction_broadcast,
      callback_transaction_mined,
      callback_transaction_mined_unconfirmed,
      callback_faux_transaction_confirmed,
      callback_faux_transaction_unconfirmed,
      callback_transaction_send_result,
      callback_transaction_cancellation,
      callback_txo_validation_complete,
      callback_contacts_liveness_data_updated,
      callback_balance_updated,
      callback_transaction_validation_complete,
      callback_saf_message_received,
      callback_connectivity_status,
      recovery_in_progress,
      error
    );
    this.checkErrorResult(error, `walletCreate`);
    return result;
  }

  static walletGetBalance(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_balance(ptr, error);
    this.checkErrorResult(error, `walletGetBalance`);
    return result;
  }

  static walletGetPublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_public_key(ptr, error);
    this.checkErrorResult(error, `walletGetPublicKey`);
    return result;
  }

  static walletSignMessage(ptr, msg) {
    let error = this.initError();
    let result = this.fn.wallet_sign_message(ptr, msg, error);
    this.checkErrorResult(error, `walletSignMessage`);
    return result;
  }

  static walletVerifyMessageSignature(ptr, public_key_ptr, hex_sig_nonce, msg) {
    let error = this.initError();
    let result = this.fn.wallet_verify_message_signature(
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
    let result = this.fn.wallet_add_base_node_peer(
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
    let result = this.fn.wallet_upsert_contact(ptr, contact_ptr, error);
    this.checkErrorResult(error, `walletUpsertContact`);
    return result;
  }

  static walletRemoveContact(ptr, contact_ptr) {
    let error = this.initError();
    let result = this.fn.wallet_remove_contact(ptr, contact_ptr, error);
    this.checkErrorResult(error, `walletRemoveContact`);
    return result;
  }

  static balanceGetAvailable(ptr) {
    let error = this.initError();
    let result = this.fn.balance_get_available(ptr, error);
    this.checkErrorResult(error, `balanceGetAvailable`);
    return result;
  }

  static balanceGetTimeLocked(ptr) {
    let error = this.initError();
    let result = this.fn.balance_get_time_locked(ptr, error);
    this.checkErrorResult(error, `balanceGetTimeLocked`);
    return result;
  }

  static balanceGetPendingIncoming(ptr) {
    let error = this.initError();
    let result = this.fn.balance_get_pending_incoming(ptr, error);
    this.checkErrorResult(error, `balanceGetPendingIncoming`);
    return result;
  }

  static balanceGetPendingOutgoing(ptr) {
    let error = this.initError();
    let result = this.fn.balance_get_pending_outgoing(ptr, error);
    this.checkErrorResult(error, `balanceGetPendingOutgoing`);
    return result;
  }

  static livenessDataGetPublicKey(ptr) {
    let error = this.initError();
    let result = this.fn.liveness_data_get_public_key(ptr, error);
    this.checkErrorResult(error, `livenessDataGetPublicKey`);
    return result;
  }

  static livenessDataGetLatency(ptr) {
    let error = this.initError();
    let result = this.fn.liveness_data_get_latency(ptr, error);
    this.checkErrorResult(error, `livenessDataGetLatency`);
    return result;
  }

  static livenessDataGetLastSeen(ptr) {
    let error = this.initError();
    let result = this.fn.liveness_data_get_last_seen(ptr, error);
    this.checkErrorResult(error, `livenessDataGetLastSeen`);
    return result;
  }

  static livenessDataGetMessageType(ptr) {
    let error = this.initError();
    let result = this.fn.liveness_data_get_message_type(ptr, error);
    this.checkErrorResult(error, `livenessDataGetMessageType`);
    return result;
  }

  static livenessDataGetOnlineStatus(ptr) {
    let error = this.initError();
    let result = this.fn.liveness_data_get_online_status(ptr, error);
    this.checkErrorResult(error, `livenessDataGetOnlineStatus`);
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
    let result = this.fn.wallet_get_fee_estimate(
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
    let result = this.fn.wallet_get_num_confirmations_required(ptr, error);
    this.checkErrorResult(error, `walletGetNumConfirmationsRequired`);
    return result;
  }

  static walletSetNumConfirmationsRequired(ptr, num) {
    let error = this.initError();
    this.fn.wallet_set_num_confirmations_required(ptr, num, error);
    this.checkErrorResult(error, `walletSetNumConfirmationsRequired`);
  }

  static walletSendTransaction(
    ptr,
    destination,
    amount,
    fee_per_gram,
    message,
    one_sided
  ) {
    let error = this.initError();
    let result = this.fn.wallet_send_transaction(
      ptr,
      destination,
      amount,
      fee_per_gram,
      message,
      one_sided,
      error
    );
    this.checkErrorResult(error, `walletSendTransaction`);
    return result;
  }

  static walletGetContacts(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_contacts(ptr, error);
    this.checkErrorResult(error, `walletGetContacts`);
    return result;
  }

  static walletGetCompletedTransactions(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_completed_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetCompletedTransactions`);
    return result;
  }

  static walletGetPendingOutboundTransactions(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_pending_outbound_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetPendingOutboundTransactions`);
    return result;
  }

  static walletGetPendingInboundTransactions(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_pending_inbound_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetPendingInboundTransactions`);
    return result;
  }

  static walletGetCancelledTransactions(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_cancelled_transactions(ptr, error);
    this.checkErrorResult(error, `walletGetCancelledTransactions`);
    return result;
  }

  static walletGetCompletedTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.fn.wallet_get_completed_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetCompletedTransactionById`);
    return result;
  }

  static walletGetPendingOutboundTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.fn.wallet_get_pending_outbound_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetPendingOutboundTransactionById`);
    return result;
  }

  static walletGetPendingInboundTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.fn.wallet_get_pending_inbound_transaction_by_id(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletGetPendingInboundTransactionById`);
    return result;
  }

  static walletGetCancelledTransactionById(ptr, transaction_id) {
    let error = this.initError();
    let result = this.fn.wallet_get_cancelled_transaction_by_id(
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
    features_ptr,
    metadata_signature_ptr,
    sender_offset_public_key_ptr,
    message
  ) {
    let error = this.initError();
    let result = this.fn.wallet_import_external_utxo_as_non_rewindable(
      ptr,
      amount,
      spending_key_ptr,
      source_public_key_ptr,
      features_ptr,
      metadata_signature_ptr,
      sender_offset_public_key_ptr,
      // script_private_key
      spending_key_ptr,
      // default Covenant
      null,
      // default EncryptedValue
      null,
      message,
      error
    );
    this.checkErrorResult(error, `walletImportUtxo`);
    return result;
  }

  static walletStartTxoValidation(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_start_txo_validation(ptr, error);
    this.checkErrorResult(error, `walletStartTxoValidation`);
    return result;
  }

  static walletStartTransactionValidation(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_start_transaction_validation(ptr, error);
    this.checkErrorResult(error, `walletStartTransactionValidation`);
    return result;
  }

  static walletGetFeePerGramStats(ptr, count) {
    let error = this.initError();
    let result = this.fn.wallet_get_fee_per_gram_stats(ptr, count, error);
    this.checkErrorResult(error, `walletGetFeePerGramStats`);
    return result;
  }

  //region FeePerGramStats (List)
  static feePerGramStatsGetLength(ptr) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stats_get_length(ptr, error);
    this.checkErrorResult(error, "feePerGramStatsGetLength");
    return result;
  }

  static feePerGramStatsGetAt(ptr, position) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stats_get_at(ptr, position, error);
    this.checkErrorResult(error, "feePerGramStatsGetAt");
    return result;
  }

  static feePerGramStatsDestroy(ptr) {
    this.fn.fee_per_gram_stats_destroy(ptr);
  }
  //endregion

  static feePerGramStatGetOrder(ptr) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stat_get_order(ptr, error);
    this.checkErrorResult(error, "feePerGramStatGetOrder");
    return result;
  }

  static feePerGramStatGetMinFeePerGram(ptr) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stat_get_min_fee_per_gram(ptr, error);
    this.checkErrorResult(error, "feePerGramStatGetMinFeePerGram");
    return result;
  }

  static feePerGramStatGetAvgFeePerGram(ptr) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stat_get_avg_fee_per_gram(ptr, error);
    this.checkErrorResult(error, "feePerGramStatGetAvgFeePerGram");
    return result;
  }

  static feePerGramStatGetMaxFeePerGram(ptr) {
    let error = this.initError();
    let result = this.fn.fee_per_gram_stat_get_max_fee_per_gram(ptr, error);
    this.checkErrorResult(error, "feePerGramStatGetMaxFeePerGram");
    return result;
  }

  static feePerGramStatDestroy(ptr) {
    this.fn.fee_per_gram_stat_destroy(ptr);
  }

  static walletRestartTransactionBroadcast(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_restart_transaction_broadcast(ptr, error);
    this.checkErrorResult(error, `walletRestartTransactionBroadcast`);
    return result;
  }

  static walletSetLowPowerMode(ptr) {
    let error = this.initError();
    this.fn.wallet_set_low_power_mode(ptr, error);
    this.checkErrorResult(error, `walletSetLowPowerMode`);
  }

  static walletSetNormalPowerMode(ptr) {
    let error = this.initError();
    this.fn.wallet_set_normal_power_mode(ptr, error);
    this.checkErrorResult(error, `walletSetNormalPowerMode`);
  }

  static walletCancelPendingTransaction(ptr, transaction_id) {
    let error = this.initError();
    let result = this.fn.wallet_cancel_pending_transaction(
      ptr,
      transaction_id,
      error
    );
    this.checkErrorResult(error, `walletCancelPendingTransaction`);
    return result;
  }

  static walletCoinSplit(ptr, amount, count, fee, msg, lock_height) {
    let error = this.initError();
    let result = this.fn.wallet_coin_split(
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
    let result = this.fn.wallet_get_seed_words(ptr, error);
    this.checkErrorResult(error, `walletGetSeedWords`);
    return result;
  }

  static walletApplyEncryption(ptr, passphrase) {
    let error = this.initError();
    this.fn.wallet_apply_encryption(ptr, passphrase, error);
    this.checkErrorResult(error, `walletApplyEncryption`);
  }

  static walletRemoveEncryption(ptr) {
    let error = this.initError();
    this.fn.wallet_remove_encryption(ptr, error);
    this.checkErrorResult(error, `walletRemoveEncryption`);
  }

  static walletSetKeyValue(ptr, key_ptr, value) {
    let error = this.initError();
    let result = this.fn.wallet_set_key_value(ptr, key_ptr, value, error);
    this.checkErrorResult(error, `walletSetKeyValue`);
    return result;
  }

  static walletGetValue(ptr, key_ptr) {
    let error = this.initError();
    let result = this.fn.wallet_get_value(ptr, key_ptr, error);
    this.checkErrorResult(error, `walletGetValue`);
    return result;
  }

  static walletClearValue(ptr, key_ptr) {
    let error = this.initError();
    let result = this.fn.wallet_clear_value(ptr, key_ptr, error);
    this.checkErrorResult(error, `walletClearValue`);
    return result;
  }

  static walletIsRecoveryInProgress(ptr) {
    let error = this.initError();
    let result = this.fn.wallet_is_recovery_in_progress(ptr, error);
    this.checkErrorResult(error, `walletIsRecoveryInProgress`);
    return result;
  }

  static walletStartRecovery(
    ptr,
    base_node_public_key_ptr,
    recovery_progress_callback
  ) {
    let error = this.initError();
    let result = this.fn.wallet_start_recovery(
      ptr,
      base_node_public_key_ptr,
      recovery_progress_callback,
      error
    );
    this.checkErrorResult(error, `walletStartRecovery`);
    return result;
  }

  static walletDestroy(ptr) {
    this.fn.wallet_destroy(ptr);
  }

  static balanceDestroy(ptr) {
    this.fn.balance_destroy(ptr);
  }

  static livenessDataDestroy(ptr) {
    this.fn.liveness_data_destroy(ptr);
  }
  //endregion
}
module.exports = InterfaceFFI;
