/**
 * Goal of this library is to provide all the functions inside the ffi library.
 * All the functions that returns error, should throw error instead. So the
 * outside code can use this as a regular javascript library (with nice code
 * completion,  * and parameter names, instead of big unknown), and not to take
 * care of the ffi stuff that's going on in here. This way, anyone doing new
 * test can avoid this ffi importing, declaration, etc... And just use it.
 */

const { expect } = require("chai");
const ffi = require("ffi-napi");
const ref = require("ref-napi");
const dateFormat = require("dateformat");
const { spawn } = require("child_process");
const fs = require("fs");

class WalletFFI {
  static byte_vector = ref.types.void;
  static byte_vector_ptr = ref.refType(this.byte_vector);
  static tari_comms_config = ref.types.void;
  static tari_comms_config_ptr = ref.refType(this.tari_comms_config);
  static tari_private_key = ref.types.void;
  static tari_private_key_ptr = ref.refType(this.tari_private_key);
  static tari_wallet = ref.types.void;
  static tari_wallet_ptr = ref.refType(this.tari_wallet);
  static tari_wallet_config = ref.types.void;
  static tari_wallet_config_ptr = ref.refType(this.tari_wallet_config);
  static tari_public_key = ref.types.void;
  static tari_public_key_ptr = ref.refType(this.tari_public_key);
  static tari_contacts = ref.types.void;
  static tari_contacts_ptr = ref.refType(this.tari_contacts);
  static tari_contact = ref.types.void;
  static tari_contact_ptr = ref.refType(this.tari_contact);
  static tari_completed_transactions = ref.types.void;
  static tari_completed_transactions_ptr = ref.refType(
    this.tari_completed_transactions
  );
  static tari_completed_transaction = ref.types.void;
  static tari_completed_transaction_ptr = ref.refType(
    this.tari_completed_transaction
  );
  static tari_pending_outbound_transactions = ref.types.void;
  static tari_pending_outbound_transactions_ptr = ref.refType(
    this.tari_pending_outbound_transactions
  );
  static tari_pending_outbound_transaction = ref.types.void;
  static tari_pending_outbound_transaction_ptr = ref.refType(
    this.tari_pending_outbound_transaction
  );
  static tari_pending_inbound_transactions = ref.types.void;
  static tari_pending_inbound_transactions_ptr = ref.refType(
    this.tari_pending_inbound_transactions
  );
  static tari_pending_inbound_transaction = ref.types.void;
  static tari_pending_inbound_transaction_ptr = ref.refType(
    this.tari_pending_inbound_transaction
  );
  static tari_transport_type = ref.types.void;
  static tari_transport_type_ptr = ref.refType(this.tari_transport_type);
  static tari_seed_words = ref.types.void;
  static tari_seed_words_ptr = ref.refType(this.tari_seed_words);
  static emoji_set = ref.types.void;
  static emoji_set_ptr = ref.refType(this.emoji_set);
  static tari_excess = ref.types.void;
  static tari_excess_ptr = ref.refType(this.tari_excess);
  static tari_excess_public_nonce = ref.types.void;
  static tari_excess_public_nonce_ptr = ref.refType(
    this.tari_excess_public_nonce
  );
  static tari_excess_signature = ref.types.void;
  static tari_excess_signature_ptr = ref.refType(this.tari_excess_signature);

  static #fn;
  static error = ref.alloc("int");
  static NULL = ref.NULL;
  static #loaded = false;
  static #ps = null;
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

  static async Init() {
    if (this.#loaded) {
      return;
    }

    this.#loaded = true;
    await this.compile();
    const outputProcess = `${process.cwd()}/temp/out/${
      process.platform === "win32" ? "" : "lib"
    }tari_wallet_ffi`;
    console.log(outputProcess);

    // Load the library
    this.#fn = ffi.Library(outputProcess, {
      // Transport Types
      transport_memory_create: [this.tari_transport_type_ptr, []],
      transport_tcp_create: [this.tari_transport_type_ptr, ["string", "int*"]],
      transport_tor_create: [
        this.tari_transport_type_ptr,
        ["string", this.byte_vector_ptr, "ushort", "string", "string", "int*"],
      ],
      transport_memory_get_address: [
        "string",
        [this.tari_transport_type_ptr, "int*"],
      ],
      transport_type_destroy: ["void", [this.tari_transport_type_ptr]],
      // Strings
      string_destroy: ["void", ["string"]],
      // ByteVector
      byte_vector_create: [this.byte_vector_ptr, ["uchar*", "uint", "int*"]],
      byte_vector_get_at: ["uchar", [this.byte_vector_ptr, "uint", "int*"]],
      byte_vector_get_length: ["uint", [this.byte_vector_ptr, "int*"]],
      byte_vector_destroy: ["void", [this.byte_vector_ptr]],
      // TariPublicKey
      public_key_create: [
        this.tari_public_key_ptr,
        [this.byte_vector_ptr, "int*"],
      ],
      public_key_get_bytes: [
        this.byte_vector_ptr,
        [this.tari_public_key_ptr, "int*"],
      ],
      public_key_from_private_key: [
        this.tari_public_key_ptr,
        [this.tari_private_key_ptr, "int*"],
      ],
      public_key_from_hex: [this.tari_public_key_ptr, ["string", "int*"]],
      public_key_destroy: ["void", [this.tari_public_key_ptr]],
      public_key_to_emoji_id: ["string", [this.tari_public_key_ptr, "int*"]],
      emoji_id_to_public_key: [this.tari_public_key_ptr, ["string", "int*"]],
      // TariPrivateKey
      private_key_create: [
        this.tari_private_key_ptr,
        [this.byte_vector_ptr, "int*"],
      ],
      private_key_generate: [this.tari_private_key_ptr, []],
      private_key_get_bytes: [
        this.byte_vector_ptr,
        [this.tari_private_key_ptr, "int*"],
      ],
      private_key_from_hex: [this.tari_private_key_ptr, ["string", "int*"]],
      private_key_destroy: ["void", [this.tari_private_key_ptr]],
      // Seed Words
      seed_words_create: [this.tari_seed_words_ptr, []],
      seed_words_get_length: ["uint", [this.tari_seed_words_ptr, "int*"]],
      seed_words_get_at: ["string", [this.tari_seed_words_ptr, "uint", "int*"]],
      seed_words_push_word: [
        "uchar",
        [this.tari_seed_words_ptr, "string", "int*"],
      ],
      seed_words_destroy: ["void", [this.tari_seed_words_ptr]],
      // Contact
      contact_create: [
        this.tari_contact_ptr,
        ["string", this.tari_public_key_ptr, "int*"],
      ],
      contact_get_alias: ["string", [this.tari_contact_ptr, "int*"]],
      contact_get_public_key: [
        this.tari_public_key_ptr,
        [this.tari_contact_ptr, "int*"],
      ],
      contact_destroy: ["void", [this.tari_contact_ptr]],
      // Contacts
      contacts_get_length: ["uint", [this.tari_contacts_ptr, "int*"]],
      contacts_get_at: [
        this.tari_contact_ptr,
        [this.tari_contacts_ptr, "uint", "int*"],
      ],
      contacts_destroy: ["void", [this.tari_contacts_ptr]],
      // CompletedTransaction
      completed_transaction_get_destination_public_key: [
        this.tari_public_key_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_source_public_key: [
        this.tari_public_key_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_amount: [
        "ulong long",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_fee: [
        "ulong long",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_message: [
        "string",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_status: [
        "int",
        [this.tari_completed_transaction, "int*"],
      ],
      completed_transaction_get_transaction_id: [
        "ulong long",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_timestamp: [
        "ulong long",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_is_valid: [
        "bool",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_is_outbound: [
        "bool",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_confirmations: [
        "ulong long",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_destroy: [
        "void",
        [this.tari_completed_transaction_ptr],
      ],
      completed_transaction_get_excess: [
        this.tari_excess_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_public_nonce: [
        this.tari_excess_public_nonce_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_signature: [
        this.tari_excess_signature_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      excess_destroy: ["void", [this.tari_excess_ptr]],
      nonce_destroy: ["void", [this.tari_excess_public_nonce_ptr]],
      signature_destroy: ["void", [this.tari_excess_signature_ptr]],
      // CompletedTransactions
      completed_transactions_get_length: [
        "uint",
        [this.tari_completed_transactions_ptr, "int*"],
      ],
      completed_transactions_get_at: [
        this.tari_completed_transaction_ptr,
        [this.tari_completed_transactions_ptr, "uint", "int*"],
      ],
      completed_transactions_destroy: [
        "void",
        [this.tari_completed_transactions_ptr],
      ],
      // OutboundTransaction
      pending_outbound_transaction_get_transaction_id: [
        "ulong long",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_destination_public_key: [
        this.tari_public_key_ptr,
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_amount: [
        "ulong long",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_fee: [
        "ulong long",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_message: [
        "string",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_timestamp: [
        "ulong long",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_status: [
        "int",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_destroy: [
        "void",
        [this.tari_pending_outbound_transaction_ptr],
      ],
      // OutboundTransactions
      pending_outbound_transactions_get_length: [
        "uint",
        [this.tari_pending_outbound_transactions_ptr, "int*"],
      ],
      pending_outbound_transactions_get_at: [
        this.tari_pending_outbound_transaction_ptr,
        [this.tari_pending_outbound_transactions_ptr, "uint", "int*"],
      ],
      pending_outbound_transactions_destroy: [
        "void",
        [this.tari_pending_outbound_transactions_ptr],
      ],
      // InboundTransaction
      pending_inbound_transaction_get_transaction_id: [
        "ulong long",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_source_public_key: [
        this.tari_public_key_ptr,
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_message: [
        "string",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_amount: [
        "ulong long",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_timestamp: [
        "ulong long",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_status: [
        "int",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_destroy: [
        "void",
        [this.tari_pending_inbound_transaction_ptr],
      ],
      // InboundTransactions
      pending_inbound_transactions_get_length: [
        "uint",
        [this.tari_pending_inbound_transactions_ptr, "int*"],
      ],
      pending_inbound_transactions_get_at: [
        this.tari_pending_inbound_transaction_ptr,
        [this.tari_pending_inbound_transactions_ptr, "uint", "int*"],
      ],
      pending_inbound_transactions_destroy: [
        "void",
        [this.tari_pending_inbound_transactions_ptr],
      ],
      // TariCommsConfig
      comms_config_create: [
        this.tari_comms_config_ptr,
        [
          "string",
          this.tari_transport_type_ptr,
          "string",
          "string",
          "ulong long",
          "ulong long",
          "int*",
        ],
      ],
      comms_config_destroy: ["void", [this.tari_comms_config_ptr]],
      // TariWallet
      wallet_create: [
        this.tari_wallet_ptr,
        [
          this.tari_wallet_config_ptr,
          "string",
          "uint",
          "uint",
          "string",
          this.tari_seed_words_ptr,
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
          "int*",
        ],
      ],
      wallet_sign_message: ["string", [this.tari_wallet_ptr, "string", "int*"]],
      wallet_verify_message_signature: [
        "bool",
        [
          this.tari_wallet_ptr,
          this.tari_public_key_ptr,
          "string",
          "string",
          "int*",
        ],
      ],
      wallet_test_generate_data: [
        "bool",
        [this.tari_wallet_ptr, "string", "int*"],
      ],
      wallet_add_base_node_peer: [
        "bool",
        [this.tari_wallet_ptr, this.tari_public_key_ptr, "string", "int*"],
      ],
      wallet_upsert_contact: [
        "bool",
        [this.tari_wallet_ptr, this.tari_contact_ptr, "int*"],
      ],
      wallet_remove_contact: [
        "bool",
        [this.tari_wallet_ptr, this.tari_contact_ptr, "int*"],
      ],
      wallet_get_available_balance: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_pending_incoming_balance: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_pending_outgoing_balance: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_fee_estimate: [
        "ulong long",
        [
          this.tari_wallet_ptr,
          "ulong long",
          "ulong long",
          "ulong long",
          "ulong long",
          "int*",
        ],
      ],
      wallet_get_num_confirmations_required: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_set_num_confirmations_required: [
        "void",
        [this.tari_wallet_ptr, "long long", "int*"],
      ],
      wallet_send_transaction: [
        "long long",
        [
          this.tari_wallet_ptr,
          this.tari_public_key_ptr,
          "ulong long",
          "ulong long",
          "string",
          "int*",
        ],
      ],
      wallet_get_contacts: [
        this.tari_contacts_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_completed_transactions: [
        this.tari_completed_transactions_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_pending_outbound_transactions: [
        this.tari_pending_outbound_transactions_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_public_key: [
        this.tari_public_key_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_pending_inbound_transactions: [
        this.tari_pending_inbound_transactions_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_cancelled_transactions: [
        this.tari_completed_transactions_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_completed_transaction_by_id: [
        this.tari_completed_transaction_ptr,
        [this.tari_wallet_ptr, "ulong long", "int*"],
      ],
      wallet_get_pending_outbound_transaction_by_id: [
        this.tari_pending_outbound_transaction_ptr,
        [this.tari_wallet_ptr, "uint", "int*"],
      ],
      wallet_get_pending_inbound_transaction_by_id: [
        this.tari_pending_outbound_transaction_ptr,
        [this.tari_wallet_ptr, "uint", "int*"],
      ],
      wallet_get_cancelled_transaction_by_id: [
        this.tari_completed_transaction_ptr,
        [this.tari_wallet_ptr, "int", "int*"],
      ],
      wallet_test_complete_sent_transaction: [
        "bool",
        [
          this.tari_wallet_ptr,
          this.tari_pending_outbound_transaction_ptr,
          "int*",
        ],
      ],
      wallet_import_utxo: [
        "ulong long",
        [
          this.tari_wallet_ptr,
          "ulong long",
          this.tari_private_key_ptr,
          this.tari_public_key_ptr,
          "string",
          "int*",
        ],
      ],
      wallet_start_utxo_validation: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_start_stxo_validation: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_start_invalid_txo_validation: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_start_transaction_validation: [
        "ulong long",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_restart_transaction_broadcast: [
        "bool",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_set_low_power_mode: ["void", [this.tari_wallet_ptr, "int*"]],
      wallet_set_normal_power_mode: ["void", [this.tari_wallet_ptr, "int*"]],
      wallet_test_broadcast_transaction: [
        "bool",
        [this.tari_wallet_ptr, "uint", "int*"],
      ],
      wallet_test_finalize_received_transaction: [
        "bool",
        [
          this.tari_wallet_ptr,
          this.tari_pending_inbound_transaction_ptr,
          "int*",
        ],
      ],
      wallet_test_mine_transaction: [
        "bool",
        [this.tari_wallet_ptr, "ulong long", "int*"],
      ],
      wallet_test_receive_transaction: ["bool", [this.tari_wallet_ptr, "int*"]],
      wallet_cancel_pending_transaction: [
        "bool",
        [this.tari_wallet_ptr, "ulong long", "int*"],
      ],
      wallet_coin_split: [
        "ulong long",
        [
          this.tari_wallet_ptr,
          "ulong long",
          "ulong long",
          "ulong long",
          "string",
          "ulong long",
          "int*",
        ],
      ],
      wallet_get_seed_words: [
        this.tari_seed_words_ptr,
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_apply_encryption: [
        "void",
        [this.tari_wallet_ptr, "string", "int*"],
      ],
      wallet_remove_encryption: ["void", [this.tari_wallet_ptr, "int*"]],
      wallet_set_key_value: [
        "bool",
        [this.tari_wallet_ptr, "string", "string", "int*"],
      ],
      wallet_get_value: ["string", [this.tari_wallet_ptr, "string", "int*"]],
      wallet_clear_value: ["bool", [this.tari_wallet_ptr, "string", "int*"]],
      wallet_is_recovery_in_progress: ["bool", [this.tari_wallet_ptr, "int*"]],
      wallet_start_recovery: [
        "bool",
        [this.tari_wallet_ptr, this.tari_public_key_ptr, "pointer", "int*"],
      ],
      wallet_destroy: ["void", [this.tari_wallet_ptr]],
      file_partial_backup: ["void", ["string", "string", "int*"]],
      log_debug_message: ["void", ["string"]],
      get_emoji_set: [this.emoji_set_ptr, []],
      emoji_set_destroy: ["void", [this.emoji_set_ptr]],
      emoji_set_get_at: [
        this.byte_vector_ptr,
        [this.emoji_set_ptr, "uint", "int*"],
      ],
      emoji_set_get_length: ["uint", [this.emoji_set_ptr, "int*"]],
    });
  }

  static checkError(text) {
    expect(this.error.deref()).to.equal(0, `Error in ${text}`);
  }

  /// -------------------------------- Transport Types ----------------------------------------------- ///

  static transportTcpCreate(listener_address) {
    let tcp = this.#fn.transport_tcp_create(listener_address, this.error);
    this.checkError("transportTcpCreate");
    return tcp;
  }

  static transportTorCreate(
    control_server_address,
    tor_cookie,
    tor_port,
    socks_username,
    socks_password
  ) {
    let tor = this.#fn.transport_tor_create(
      control_server_address,
      tor_cookie,
      tor_port,
      socks_username,
      socks_password,
      this.error
    );
    this.checkError("transportTorCreate");
    return tor;
  }

  static transportMemoryGetAddress(transport) {
    let address = this.#fn.transport_memory_get_address(transport, this.error);
    this.checkError("transportMemoryGetAddress");
    return address;
  }

  static transportTypeDestroy(transport) {
    this.#fn.transport_type_destroy(transport);
  }

  /// -------------------------------- Strings ----------------------------------------------- ///

  static stringDestroy(string) {
    this.#fn.string_destroy(string);
  }

  /// -------------------------------- ByteVector ----------------------------------------------- ///

  static byteVectorCreate(byte_array, element_count) {
    const byte_vector = this.#fn.byte_vector_create(
      byte_array,
      element_count,
      this.error
    );
    this.checkError("byteVectorCreate");
    /// -------------------------------- TariWallet ----------------------------------------------- //
    return byte_vector;
  }

  static byteVectorGetAt(byte_array, i) {
    const byte = this.#fn.byte_vector_get_at(byte_array, i, this.error);
    this.checkError("byteVectorGetAt");
    return byte;
  }

  static byteVectorGetLength(byte_array) {
    const length = this.#fn.byte_vector_get_length(byte_array, this.error);
    this.checkError("byteVectorGetLength");
    return length;
  }

  static byteVectorDestroy(byte_array) {
    this.#fn.byte_vector_destroy(byte_array);
  }

  /// -------------------------------- TariPublicKey ----------------------------------------------- ///

  static publicKeyGetBytes(public_key) {
    let bytes = this.#fn.public_key_get_bytes(public_key, this.error);
    this.checkError("publicKeyGetBytes");
    return bytes;
  }

  static publicKeyDestroy(public_key) {
    this.#fn.public_key_destroy(public_key);
  }

  static publicKeyToEmojiId(public_key) {
    let emoji_id = this.#fn.public_key_to_emoji_id(public_key, this.error);
    this.checkError("publicKeyToEmojiId");
    return emoji_id;
  }
  /// -------------------------------- TariPrivateKey ----------------------------------------------- ///
  /// -------------------------------- Seed Words  -------------------------------------------------- ///
  /// -------------------------------- Contact ------------------------------------------------------ ///
  /// -------------------------------- Contacts ------------------------------------------------------ ///
  /// -------------------------------- CompletedTransaction ------------------------------------------------------ ///
  /// -------------------------------- CompletedTransactions ------------------------------------------------------ ///
  /// -------------------------------- OutboundTransaction ------------------------------------------------------ ///
  /// -------------------------------- OutboundTransactions ------------------------------------------------------ ///
  /// -------------------------------- InboundTransaction ------------------------------------------------------ ///
  /// -------------------------------- InboundTransactions ------------------------------------------------------ ///
  /// -------------------------------- TariCommsConfig ----------------------------------------------- ///

  static commsConfigCreate(
    public_address,
    transport,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs
  ) {
    let comms_config = this.#fn.comms_config_create(
      public_address,
      transport,
      database_name,
      datastore_path,
      discovery_timeout_in_secs,
      saf_message_duration_in_secs,
      this.error
    );
    this.checkError("commsConfigCreate");
    return comms_config;
  }

  /// -------------------------------- TariWallet ----------------------------------------------- //

  static walletCreate(
    comms_config,
    log_path,
    num_rolling_log_files,
    size_per_log_file_bytes,
    passphrase,
    seed_words
    // callback_received_transaction
  ) {
    let callback_received_transaction = ffi.Callback(
      "void",
      [this.tari_pending_inbound_transaction_ptr],
      function (_pending_inbound_transcation) {
        console.log("callback_received_transaction");
      }
    );
    let callback_received_transaction_reply = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr],
      function (_completed_transaction) {
        console.log("callback_received_transaction_reply");
      }
    );
    let callback_received_finalized_transaction = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr],
      function (_completed_transaction) {
        console.log("callback_received_finalized_transaction");
      }
    );
    let callback_transaction_broadcast = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr],
      function (_completed_transaction) {
        console.log("callback_transaction_broadcast");
      }
    );
    let callback_transaction_mined = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr],
      function (_completed_transaction) {
        console.log("callback_transaction_mined");
      }
    );
    let callback_transaction_mined_unconfirmed = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr, "long long"],
      function (_completed_transaction, x) {
        console.log("callback_transaction_mined_unconfirmed", x);
      }
    );
    let callback_direct_send_result = ffi.Callback(
      "void",
      ["long long", "bool"],
      function (x, y) {
        console.log("callback_direct_send_result", x, y);
      }
    );
    let callback_store_and_forward_send_result = ffi.Callback(
      "void",
      ["long long", "bool"],
      function (x, y) {
        console.log("callback_store_and_forward_send_result", x, y);
      }
    );
    let callback_transaction_cancellation = ffi.Callback(
      "void",
      [this.tari_completed_transaction_ptr],
      function (_completed_transaction) {
        console.log("callback_transaction_cancellation");
      }
    );
    let callback_utxo_validation_complete = ffi.Callback(
      "void",
      ["long long", "char"],
      function (x, y) {
        console.log("callback_utxo_validation_complete", x, y);
      }
    );
    let callback_stxo_validation_complete = ffi.Callback(
      "void",
      ["long long", "char"],
      function (x, y) {
        console.log("callback_stxo_validation_complete", x, y);
      }
    );
    let callback_invalid_txo_validation_complete = ffi.Callback(
      "void",
      ["long long", "char"],
      function (x, y) {
        console.log("callback_invalid_txo_validation_complete", x, y);
      }
    );
    let callback_transaction_validation_complete = ffi.Callback(
      "void",
      ["long long", "char"],
      function (x, y) {
        console.log("callback_transaction_validation_complete", x, y);
      }
    );
    let callback_saf_message_received = ffi.Callback("void", [], function () {
      console.log("callback_saf_message_received");
    });

    let wallet = this.#fn.wallet_create(
      comms_config,
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
      this.error
    );

    this.checkError("walletCreate");
    return wallet;
  }

  static walletGetPublicKey(wallet) {
    const public_key = this.#fn.wallet_get_public_key(wallet, this.error);
    this.checkError("walletGetPublicKey");
    return public_key;
  }

  static walletDestroy(wallet) {
    this.#fn.wallet_destroy(wallet);
  }
}

module.exports = WalletFFI;
