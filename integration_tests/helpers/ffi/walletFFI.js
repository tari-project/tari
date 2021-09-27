/**
 * This library was AUTO-GENERATED. Do not modify manually!
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
  static error = ref.alloc(ref.types.int);
  static recovery_in_progress = ref.alloc(ref.types.bool);
  static NULL = ref.NULL;
  static #loaded = false;
  static #ps = null;

  static checkAsyncRes(resolve, reject, error_name) {
    return (err, res) => {
      if (err) reject(err);
      expect(this.error.deref()).to.equal(0, `Error in ${error_name}`);
      resolve(res);
    };
  }

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

    // Init callbacks

    this.createCallbackReceivedTransaction = (callback) =>
      ffi.Callback(
        "void",
        [this.tari_pending_inbound_transaction_ptr],
        callback
      );
    this.createCallbackReceivedTransactionReply = (callback) =>
      ffi.Callback("void", [this.tari_completed_transaction_ptr], callback);
    this.createCallbackReceivedFinalizedTransaction = (callback) =>
      ffi.Callback("void", [this.tari_completed_transaction_ptr], callback);
    this.createCallbackTransactionBroadcast = (callback) =>
      ffi.Callback("void", [this.tari_completed_transaction_ptr], callback);
    this.createCallbackTransactionMined = (callback) =>
      ffi.Callback("void", [this.tari_completed_transaction_ptr], callback);
    this.createCallbackTransactionMinedUnconfirmed = (callback) =>
      ffi.Callback(
        "void",
        [this.tari_completed_transaction_ptr, "uint64"],
        callback
      );
    this.createCallbackDirectSendResult = (callback) =>
      ffi.Callback("void", ["uint64", "bool"], callback);
    this.createCallbackStoreAndForwardSendResult = (callback) =>
      ffi.Callback("void", ["uint64", "bool"], callback);
    this.createCallbackTransactionCancellation = (callback) =>
      ffi.Callback("void", [this.tari_completed_transaction_ptr], callback);
    this.createCallbackUtxoValidationComplete = (callback) =>
      ffi.Callback("void", ["uint64", "uchar"], callback);
    this.createCallbackStxoValidationComplete = (callback) =>
      ffi.Callback("void", ["uint64", "uchar"], callback);
    this.createCallbackInvalidTxoValidationComplete = (callback) =>
      ffi.Callback("void", ["uint64", "uchar"], callback);
    this.createCallbackTransactionValidationComplete = (callback) =>
      ffi.Callback("void", ["uint64", "uchar"], callback);
    this.createCallbackSafMessageReceived = (callback) =>
      ffi.Callback("void", [], callback);
    this.createRecoveryProgressCallback = (callback) =>
      ffi.Callback("void", ["uchar", "uint64", "uint64"], callback);
    // Load the library
    this.#fn = ffi.Library(outputProcess, {
      transport_memory_create: [this.tari_transport_type_ptr, []],
      transport_tcp_create: [this.tari_transport_type_ptr, ["string", "int*"]],
      transport_tor_create: [
        this.tari_transport_type_ptr,
        ["string", this.byte_vector_ptr, "ushort", "string", "string", "int*"],
      ],
      transport_memory_get_address: [
        "char*",
        [this.tari_transport_type_ptr, "int*"],
      ],
      transport_type_destroy: ["void", [this.tari_transport_type_ptr]],
      string_destroy: ["void", ["string"]],
      byte_vector_create: [this.byte_vector_ptr, ["uchar*", "uint", "int*"]],
      byte_vector_get_at: ["uchar", [this.byte_vector_ptr, "uint", "int*"]],
      byte_vector_get_length: ["uint", [this.byte_vector_ptr, "int*"]],
      byte_vector_destroy: ["void", [this.byte_vector_ptr]],
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
      public_key_to_emoji_id: ["char*", [this.tari_public_key_ptr, "int*"]],
      emoji_id_to_public_key: [this.tari_public_key_ptr, ["string", "int*"]],
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
      seed_words_create: [this.tari_seed_words_ptr, []],
      seed_words_get_length: ["uint", [this.tari_seed_words_ptr, "int*"]],
      seed_words_get_at: ["char*", [this.tari_seed_words_ptr, "uint", "int*"]],
      seed_words_push_word: [
        "uchar",
        [this.tari_seed_words_ptr, "string", "int*"],
      ],
      seed_words_destroy: ["void", [this.tari_seed_words_ptr]],
      contact_create: [
        this.tari_contact_ptr,
        ["string", this.tari_public_key_ptr, "int*"],
      ],
      contact_get_alias: ["char*", [this.tari_contact_ptr, "int*"]],
      contact_get_public_key: [
        this.tari_public_key_ptr,
        [this.tari_contact_ptr, "int*"],
      ],
      contact_destroy: ["void", [this.tari_contact_ptr]],
      contacts_get_length: ["uint", [this.tari_contacts_ptr, "int*"]],
      contacts_get_at: [
        this.tari_contact_ptr,
        [this.tari_contacts_ptr, "uint", "int*"],
      ],
      contacts_destroy: ["void", [this.tari_contacts_ptr]],
      completed_transaction_get_destination_public_key: [
        this.tari_public_key_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_source_public_key: [
        this.tari_public_key_ptr,
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_amount: [
        "uint64",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_fee: [
        "uint64",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_message: [
        "char*",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_status: [
        "int",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_transaction_id: [
        "uint64",
        [this.tari_completed_transaction_ptr, "int*"],
      ],
      completed_transaction_get_timestamp: [
        "uint64",
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
        "uint64",
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
      pending_outbound_transaction_get_transaction_id: [
        "uint64",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_destination_public_key: [
        this.tari_public_key_ptr,
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_amount: [
        "uint64",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_fee: [
        "uint64",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_message: [
        "char*",
        [this.tari_pending_outbound_transaction_ptr, "int*"],
      ],
      pending_outbound_transaction_get_timestamp: [
        "uint64",
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
      pending_inbound_transaction_get_transaction_id: [
        "uint64",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_source_public_key: [
        this.tari_public_key_ptr,
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_message: [
        "char*",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_amount: [
        "uint64",
        [this.tari_pending_inbound_transaction_ptr, "int*"],
      ],
      pending_inbound_transaction_get_timestamp: [
        "uint64",
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
      comms_config_create: [
        this.tari_comms_config_ptr,
        [
          "string",
          this.tari_transport_type_ptr,
          "string",
          "string",
          "uint64",
          "uint64",
          "string",
          "int*",
        ],
      ],
      comms_config_destroy: ["void", [this.tari_comms_config_ptr]],
      wallet_create: [
        this.tari_wallet_ptr,
        [
          this.tari_comms_config_ptr,
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
          "bool*",
          "int*",
        ],
      ],
      wallet_sign_message: ["char*", [this.tari_wallet_ptr, "string", "int*"]],
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
      wallet_get_available_balance: ["uint64", [this.tari_wallet_ptr, "int*"]],
      wallet_get_pending_incoming_balance: [
        "uint64",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_pending_outgoing_balance: [
        "uint64",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_get_fee_estimate: [
        "uint64",
        [this.tari_wallet_ptr, "uint64", "uint64", "uint64", "uint64", "int*"],
      ],
      wallet_get_num_confirmations_required: [
        "uint64",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_set_num_confirmations_required: [
        "void",
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_send_transaction: [
        "uint64",
        [
          this.tari_wallet_ptr,
          this.tari_public_key_ptr,
          "uint64",
          "uint64",
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
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_get_pending_outbound_transaction_by_id: [
        this.tari_pending_outbound_transaction_ptr,
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_get_pending_inbound_transaction_by_id: [
        this.tari_pending_inbound_transaction_ptr,
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_get_cancelled_transaction_by_id: [
        this.tari_completed_transaction_ptr,
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_import_utxo: [
        "uint64",
        [
          this.tari_wallet_ptr,
          "uint64",
          this.tari_private_key_ptr,
          this.tari_public_key_ptr,
          "string",
          "int*",
        ],
      ],
      wallet_start_txo_validation: ["uint64", [this.tari_wallet_ptr, "int*"]],
      wallet_start_transaction_validation: [
        "uint64",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_restart_transaction_broadcast: [
        "bool",
        [this.tari_wallet_ptr, "int*"],
      ],
      wallet_set_low_power_mode: ["void", [this.tari_wallet_ptr, "int*"]],
      wallet_set_normal_power_mode: ["void", [this.tari_wallet_ptr, "int*"]],
      wallet_cancel_pending_transaction: [
        "bool",
        [this.tari_wallet_ptr, "uint64", "int*"],
      ],
      wallet_coin_split: [
        "uint64",
        [
          this.tari_wallet_ptr,
          "uint64",
          "uint64",
          "uint64",
          "string",
          "uint64",
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
      wallet_get_value: ["char*", [this.tari_wallet_ptr, "string", "int*"]],
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

  static transportMemoryCreate() {
    return new Promise((resolve, reject) =>
      this.#fn.transport_memory_create.async(
        this.checkAsyncRes(resolve, reject, "transportMemoryCreate")
      )
    );
  }

  static transportTcpCreate(listener_address) {
    return new Promise((resolve, reject) =>
      this.#fn.transport_tcp_create.async(
        listener_address,
        this.error,
        this.checkAsyncRes(resolve, reject, "transportTcpCreate")
      )
    );
  }

  static transportTorCreate(
    control_server_address,
    tor_cookie,
    tor_port,
    socks_username,
    socks_password
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.transport_tor_create.async(
        control_server_address,
        tor_cookie,
        tor_port,
        socks_username,
        socks_password,
        this.error,
        this.checkAsyncRes(resolve, reject, "transportTorCreate")
      )
    );
  }

  static transportMemoryGetAddress(transport) {
    return new Promise((resolve, reject) =>
      this.#fn.transport_memory_get_address.async(
        transport,
        this.error,
        this.checkAsyncRes(resolve, reject, "transportMemoryGetAddress")
      )
    );
  }

  static transportTypeDestroy(transport) {
    return new Promise((resolve, reject) =>
      this.#fn.transport_type_destroy.async(
        transport,
        this.checkAsyncRes(resolve, reject, "transportTypeDestroy")
      )
    );
  }

  static stringDestroy(s) {
    return new Promise((resolve, reject) =>
      this.#fn.string_destroy.async(
        s,
        this.checkAsyncRes(resolve, reject, "stringDestroy")
      )
    );
  }

  static byteVectorCreate(byte_array, element_count) {
    return new Promise((resolve, reject) =>
      this.#fn.byte_vector_create.async(
        byte_array,
        element_count,
        this.error,
        this.checkAsyncRes(resolve, reject, "byteVectorCreate")
      )
    );
  }

  static byteVectorGetAt(ptr, i) {
    return new Promise((resolve, reject) =>
      this.#fn.byte_vector_get_at.async(
        ptr,
        i,
        this.error,
        this.checkAsyncRes(resolve, reject, "byteVectorGetAt")
      )
    );
  }

  static byteVectorGetLength(vec) {
    return new Promise((resolve, reject) =>
      this.#fn.byte_vector_get_length.async(
        vec,
        this.error,
        this.checkAsyncRes(resolve, reject, "byteVectorGetLength")
      )
    );
  }

  static byteVectorDestroy(bytes) {
    return new Promise((resolve, reject) =>
      this.#fn.byte_vector_destroy.async(
        bytes,
        this.checkAsyncRes(resolve, reject, "byteVectorDestroy")
      )
    );
  }

  static publicKeyCreate(bytes) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_create.async(
        bytes,
        this.error,
        this.checkAsyncRes(resolve, reject, "publicKeyCreate")
      )
    );
  }

  static publicKeyGetBytes(public_key) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_get_bytes.async(
        public_key,
        this.error,
        this.checkAsyncRes(resolve, reject, "publicKeyGetBytes")
      )
    );
  }

  static publicKeyFromPrivateKey(secret_key) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_from_private_key.async(
        secret_key,
        this.error,
        this.checkAsyncRes(resolve, reject, "publicKeyFromPrivateKey")
      )
    );
  }

  static publicKeyFromHex(hex) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_from_hex.async(
        hex,
        this.error,
        this.checkAsyncRes(resolve, reject, "publicKeyFromHex")
      )
    );
  }

  static publicKeyDestroy(pk) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_destroy.async(
        pk,
        this.checkAsyncRes(resolve, reject, "publicKeyDestroy")
      )
    );
  }

  static publicKeyToEmojiId(pk) {
    return new Promise((resolve, reject) =>
      this.#fn.public_key_to_emoji_id.async(
        pk,
        this.error,
        this.checkAsyncRes(resolve, reject, "publicKeyToEmojiId")
      )
    );
  }

  static emojiIdToPublicKey(emoji) {
    return new Promise((resolve, reject) =>
      this.#fn.emoji_id_to_public_key.async(
        emoji,
        this.error,
        this.checkAsyncRes(resolve, reject, "emojiIdToPublicKey")
      )
    );
  }

  static privateKeyCreate(bytes) {
    return new Promise((resolve, reject) =>
      this.#fn.private_key_create.async(
        bytes,
        this.error,
        this.checkAsyncRes(resolve, reject, "privateKeyCreate")
      )
    );
  }

  static privateKeyGenerate() {
    return new Promise((resolve, reject) =>
      this.#fn.private_key_generate.async(
        this.checkAsyncRes(resolve, reject, "privateKeyGenerate")
      )
    );
  }

  static privateKeyGetBytes(private_key) {
    return new Promise((resolve, reject) =>
      this.#fn.private_key_get_bytes.async(
        private_key,
        this.error,
        this.checkAsyncRes(resolve, reject, "privateKeyGetBytes")
      )
    );
  }

  static privateKeyFromHex(hex) {
    return new Promise((resolve, reject) =>
      this.#fn.private_key_from_hex.async(
        hex,
        this.error,
        this.checkAsyncRes(resolve, reject, "privateKeyFromHex")
      )
    );
  }

  static privateKeyDestroy(pk) {
    return new Promise((resolve, reject) =>
      this.#fn.private_key_destroy.async(
        pk,
        this.checkAsyncRes(resolve, reject, "privateKeyDestroy")
      )
    );
  }

  static seedWordsCreate() {
    return new Promise((resolve, reject) =>
      this.#fn.seed_words_create.async(
        this.checkAsyncRes(resolve, reject, "seedWordsCreate")
      )
    );
  }

  static seedWordsGetLength(seed_words) {
    return new Promise((resolve, reject) =>
      this.#fn.seed_words_get_length.async(
        seed_words,
        this.error,
        this.checkAsyncRes(resolve, reject, "seedWordsGetLength")
      )
    );
  }

  static seedWordsGetAt(seed_words, position) {
    return new Promise((resolve, reject) =>
      this.#fn.seed_words_get_at.async(
        seed_words,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "seedWordsGetAt")
      )
    );
  }

  static seedWordsPushWord(seed_words, word) {
    return new Promise((resolve, reject) =>
      this.#fn.seed_words_push_word.async(
        seed_words,
        word,
        this.error,
        this.checkAsyncRes(resolve, reject, "seedWordsPushWord")
      )
    );
  }

  static seedWordsDestroy(seed_words) {
    return new Promise((resolve, reject) =>
      this.#fn.seed_words_destroy.async(
        seed_words,
        this.checkAsyncRes(resolve, reject, "seedWordsDestroy")
      )
    );
  }

  static contactCreate(alias, public_key) {
    return new Promise((resolve, reject) =>
      this.#fn.contact_create.async(
        alias,
        public_key,
        this.error,
        this.checkAsyncRes(resolve, reject, "contactCreate")
      )
    );
  }

  static contactGetAlias(contact) {
    return new Promise((resolve, reject) =>
      this.#fn.contact_get_alias.async(
        contact,
        this.error,
        this.checkAsyncRes(resolve, reject, "contactGetAlias")
      )
    );
  }

  static contactGetPublicKey(contact) {
    return new Promise((resolve, reject) =>
      this.#fn.contact_get_public_key.async(
        contact,
        this.error,
        this.checkAsyncRes(resolve, reject, "contactGetPublicKey")
      )
    );
  }

  static contactDestroy(contact) {
    return new Promise((resolve, reject) =>
      this.#fn.contact_destroy.async(
        contact,
        this.checkAsyncRes(resolve, reject, "contactDestroy")
      )
    );
  }

  static contactsGetLength(contacts) {
    return new Promise((resolve, reject) =>
      this.#fn.contacts_get_length.async(
        contacts,
        this.error,
        this.checkAsyncRes(resolve, reject, "contactsGetLength")
      )
    );
  }

  static contactsGetAt(contacts, position) {
    return new Promise((resolve, reject) =>
      this.#fn.contacts_get_at.async(
        contacts,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "contactsGetAt")
      )
    );
  }

  static contactsDestroy(contacts) {
    return new Promise((resolve, reject) =>
      this.#fn.contacts_destroy.async(
        contacts,
        this.checkAsyncRes(resolve, reject, "contactsDestroy")
      )
    );
  }

  static completedTransactionGetDestinationPublicKey(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_destination_public_key.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "completedTransactionGetDestinationPublicKey"
        )
      )
    );
  }

  static completedTransactionGetSourcePublicKey(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_source_public_key.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "completedTransactionGetSourcePublicKey"
        )
      )
    );
  }

  static completedTransactionGetAmount(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_amount.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetAmount")
      )
    );
  }

  static completedTransactionGetFee(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_fee.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetFee")
      )
    );
  }

  static completedTransactionGetMessage(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_message.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetMessage")
      )
    );
  }

  static completedTransactionGetStatus(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_status.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetStatus")
      )
    );
  }

  static completedTransactionGetTransactionId(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_transaction_id.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "completedTransactionGetTransactionId"
        )
      )
    );
  }

  static completedTransactionGetTimestamp(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_timestamp.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionGetTimestamp")
      )
    );
  }

  static completedTransactionIsValid(tx) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_is_valid.async(
        tx,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionIsValid")
      )
    );
  }

  static completedTransactionIsOutbound(tx) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_is_outbound.async(
        tx,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionIsOutbound")
      )
    );
  }

  static completedTransactionGetConfirmations(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_get_confirmations.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "completedTransactionGetConfirmations"
        )
      )
    );
  }

  static completedTransactionDestroy(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transaction_destroy.async(
        transaction,
        this.checkAsyncRes(resolve, reject, "completedTransactionDestroy")
      )
    );
  }

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

  static completedTransactionsGetLength(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transactions_get_length.async(
        transactions,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionsGetLength")
      )
    );
  }

  static completedTransactionsGetAt(transactions, position) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transactions_get_at.async(
        transactions,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "completedTransactionsGetAt")
      )
    );
  }

  static completedTransactionsDestroy(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.completed_transactions_destroy.async(
        transactions,
        this.checkAsyncRes(resolve, reject, "completedTransactionsDestroy")
      )
    );
  }

  static pendingOutboundTransactionGetTransactionId(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_transaction_id.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetTransactionId"
        )
      )
    );
  }

  static pendingOutboundTransactionGetDestinationPublicKey(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_destination_public_key.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetDestinationPublicKey"
        )
      )
    );
  }

  static pendingOutboundTransactionGetAmount(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_amount.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetAmount"
        )
      )
    );
  }

  static pendingOutboundTransactionGetFee(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_fee.async(
        transaction,
        this.error,
        this.checkAsyncRes(resolve, reject, "pendingOutboundTransactionGetFee")
      )
    );
  }

  static pendingOutboundTransactionGetMessage(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_message.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetMessage"
        )
      )
    );
  }

  static pendingOutboundTransactionGetTimestamp(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_timestamp.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetTimestamp"
        )
      )
    );
  }

  static pendingOutboundTransactionGetStatus(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_get_status.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionGetStatus"
        )
      )
    );
  }

  static pendingOutboundTransactionDestroy(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transaction_destroy.async(
        transaction,
        this.checkAsyncRes(resolve, reject, "pendingOutboundTransactionDestroy")
      )
    );
  }

  static pendingOutboundTransactionsGetLength(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transactions_get_length.async(
        transactions,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionsGetLength"
        )
      )
    );
  }

  static pendingOutboundTransactionsGetAt(transactions, position) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transactions_get_at.async(
        transactions,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "pendingOutboundTransactionsGetAt")
      )
    );
  }

  static pendingOutboundTransactionsDestroy(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_outbound_transactions_destroy.async(
        transactions,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingOutboundTransactionsDestroy"
        )
      )
    );
  }

  static pendingInboundTransactionGetTransactionId(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_transaction_id.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetTransactionId"
        )
      )
    );
  }

  static pendingInboundTransactionGetSourcePublicKey(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_source_public_key.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetSourcePublicKey"
        )
      )
    );
  }

  static pendingInboundTransactionGetMessage(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_message.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetMessage"
        )
      )
    );
  }

  static pendingInboundTransactionGetAmount(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_amount.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetAmount"
        )
      )
    );
  }

  static pendingInboundTransactionGetTimestamp(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_timestamp.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetTimestamp"
        )
      )
    );
  }

  static pendingInboundTransactionGetStatus(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_get_status.async(
        transaction,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionGetStatus"
        )
      )
    );
  }

  static pendingInboundTransactionDestroy(transaction) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transaction_destroy.async(
        transaction,
        this.checkAsyncRes(resolve, reject, "pendingInboundTransactionDestroy")
      )
    );
  }

  static pendingInboundTransactionsGetLength(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transactions_get_length.async(
        transactions,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "pendingInboundTransactionsGetLength"
        )
      )
    );
  }

  static pendingInboundTransactionsGetAt(transactions, position) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transactions_get_at.async(
        transactions,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "pendingInboundTransactionsGetAt")
      )
    );
  }

  static pendingInboundTransactionsDestroy(transactions) {
    return new Promise((resolve, reject) =>
      this.#fn.pending_inbound_transactions_destroy.async(
        transactions,
        this.checkAsyncRes(resolve, reject, "pendingInboundTransactionsDestroy")
      )
    );
  }

  static commsConfigCreate(
    public_address,
    transport,
    database_name,
    datastore_path,
    discovery_timeout_in_secs,
    saf_message_duration_in_secs,
    network
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.comms_config_create.async(
        public_address,
        transport,
        database_name,
        datastore_path,
        discovery_timeout_in_secs,
        saf_message_duration_in_secs,
        network,
        this.error,
        this.checkAsyncRes(resolve, reject, "commsConfigCreate")
      )
    );
  }

  static commsConfigDestroy(wc) {
    return new Promise((resolve, reject) =>
      this.#fn.comms_config_destroy.async(
        wc,
        this.checkAsyncRes(resolve, reject, "commsConfigDestroy")
      )
    );
  }

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
    return new Promise((resolve, reject) =>
      this.#fn.wallet_create.async(
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
        this.recovery_in_progress,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletCreate")
      )
    );
  }

  static walletSignMessage(wallet, msg) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_sign_message.async(
        wallet,
        msg,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSignMessage")
      )
    );
  }

  static walletVerifyMessageSignature(wallet, public_key, hex_sig_nonce, msg) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_verify_message_signature.async(
        wallet,
        public_key,
        hex_sig_nonce,
        msg,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletVerifyMessageSignature")
      )
    );
  }

  static walletAddBaseNodePeer(wallet, public_key, address) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_add_base_node_peer.async(
        wallet,
        public_key,
        address,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletAddBaseNodePeer")
      )
    );
  }

  static walletUpsertContact(wallet, contact) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_upsert_contact.async(
        wallet,
        contact,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletUpsertContact")
      )
    );
  }

  static walletRemoveContact(wallet, contact) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_remove_contact.async(
        wallet,
        contact,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletRemoveContact")
      )
    );
  }

  static walletGetAvailableBalance(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_available_balance.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetAvailableBalance")
      )
    );
  }

  static walletGetPendingIncomingBalance(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_incoming_balance.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetPendingIncomingBalance")
      )
    );
  }

  static walletGetPendingOutgoingBalance(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_outgoing_balance.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetPendingOutgoingBalance")
      )
    );
  }

  static walletGetFeeEstimate(
    wallet,
    amount,
    fee_per_gram,
    num_kernels,
    num_outputs
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_fee_estimate.async(
        wallet,
        amount,
        fee_per_gram,
        num_kernels,
        num_outputs,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetFeeEstimate")
      )
    );
  }

  static walletGetNumConfirmationsRequired(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_num_confirmations_required.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetNumConfirmationsRequired")
      )
    );
  }

  static walletSetNumConfirmationsRequired(wallet, num) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_set_num_confirmations_required.async(
        wallet,
        num,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSetNumConfirmationsRequired")
      )
    );
  }

  static walletSendTransaction(
    wallet,
    destination,
    amount,
    fee_per_gram,
    message
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_send_transaction.async(
        wallet,
        destination,
        amount,
        fee_per_gram,
        message,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSendTransaction")
      )
    );
  }

  static walletGetContacts(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_contacts.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetContacts")
      )
    );
  }

  static walletGetCompletedTransactions(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_completed_transactions.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetCompletedTransactions")
      )
    );
  }

  static walletGetPendingOutboundTransactions(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_outbound_transactions.async(
        wallet,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "walletGetPendingOutboundTransactions"
        )
      )
    );
  }

  static walletGetPublicKey(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_public_key.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetPublicKey")
      )
    );
  }

  static walletGetPendingInboundTransactions(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_inbound_transactions.async(
        wallet,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "walletGetPendingInboundTransactions"
        )
      )
    );
  }

  static walletGetCancelledTransactions(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_cancelled_transactions.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetCancelledTransactions")
      )
    );
  }

  static walletGetCompletedTransactionById(wallet, transaction_id) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_completed_transaction_by_id.async(
        wallet,
        transaction_id,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetCompletedTransactionById")
      )
    );
  }

  static walletGetPendingOutboundTransactionById(wallet, transaction_id) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_outbound_transaction_by_id.async(
        wallet,
        transaction_id,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "walletGetPendingOutboundTransactionById"
        )
      )
    );
  }

  static walletGetPendingInboundTransactionById(wallet, transaction_id) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_pending_inbound_transaction_by_id.async(
        wallet,
        transaction_id,
        this.error,
        this.checkAsyncRes(
          resolve,
          reject,
          "walletGetPendingInboundTransactionById"
        )
      )
    );
  }

  static walletGetCancelledTransactionById(wallet, transaction_id) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_cancelled_transaction_by_id.async(
        wallet,
        transaction_id,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetCancelledTransactionById")
      )
    );
  }

  static walletImportUtxo(
    wallet,
    amount,
    spending_key,
    source_public_key,
    message
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_import_utxo.async(
        wallet,
        amount,
        spending_key,
        source_public_key,
        message,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletImportUtxo")
      )
    );
  }

  static walletStartTxoValidation(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_start_txo_validation.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletStartTxoValidation")
      )
    );
  }

  static walletStartTransactionValidation(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_start_transaction_validation.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletStartTransactionValidation")
      )
    );
  }

  static walletRestartTransactionBroadcast(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_restart_transaction_broadcast.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletRestartTransactionBroadcast")
      )
    );
  }

  static walletSetLowPowerMode(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_set_low_power_mode.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSetLowPowerMode")
      )
    );
  }

  static walletSetNormalPowerMode(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_set_normal_power_mode.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSetNormalPowerMode")
      )
    );
  }

  static walletCancelPendingTransaction(wallet, transaction_id) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_cancel_pending_transaction.async(
        wallet,
        transaction_id,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletCancelPendingTransaction")
      )
    );
  }

  static walletCoinSplit(wallet, amount, count, fee, msg, lock_height) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_coin_split.async(
        wallet,
        amount,
        count,
        fee,
        msg,
        lock_height,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletCoinSplit")
      )
    );
  }

  static walletGetSeedWords(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_seed_words.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetSeedWords")
      )
    );
  }

  static walletApplyEncryption(wallet, passphrase) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_apply_encryption.async(
        wallet,
        passphrase,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletApplyEncryption")
      )
    );
  }

  static walletRemoveEncryption(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_remove_encryption.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletRemoveEncryption")
      )
    );
  }

  static walletSetKeyValue(wallet, key, value) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_set_key_value.async(
        wallet,
        key,
        value,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletSetKeyValue")
      )
    );
  }

  static walletGetValue(wallet, key) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_get_value.async(
        wallet,
        key,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletGetValue")
      )
    );
  }

  static walletClearValue(wallet, key) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_clear_value.async(
        wallet,
        key,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletClearValue")
      )
    );
  }

  static walletIsRecoveryInProgress(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_is_recovery_in_progress.async(
        wallet,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletIsRecoveryInProgress")
      )
    );
  }

  static walletStartRecovery(
    wallet,
    base_node_public_key,
    recovery_progress_callback
  ) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_start_recovery.async(
        wallet,
        base_node_public_key,
        recovery_progress_callback,
        this.error,
        this.checkAsyncRes(resolve, reject, "walletStartRecovery")
      )
    );
  }

  static walletDestroy(wallet) {
    return new Promise((resolve, reject) =>
      this.#fn.wallet_destroy.async(
        wallet,
        this.checkAsyncRes(resolve, reject, "walletDestroy")
      )
    );
  }

  static filePartialBackup(original_file_path, backup_file_path) {
    return new Promise((resolve, reject) =>
      this.#fn.file_partial_backup.async(
        original_file_path,
        backup_file_path,
        this.error,
        this.checkAsyncRes(resolve, reject, "filePartialBackup")
      )
    );
  }

  static logDebugMessage(msg) {
    return new Promise((resolve, reject) =>
      this.#fn.log_debug_message.async(
        msg,
        this.checkAsyncRes(resolve, reject, "logDebugMessage")
      )
    );
  }

  static getEmojiSet() {
    return new Promise((resolve, reject) =>
      this.#fn.get_emoji_set.async(
        this.checkAsyncRes(resolve, reject, "getEmojiSet")
      )
    );
  }

  static emojiSetDestroy(emoji_set) {
    return new Promise((resolve, reject) =>
      this.#fn.emoji_set_destroy.async(
        emoji_set,
        this.checkAsyncRes(resolve, reject, "emojiSetDestroy")
      )
    );
  }

  static emojiSetGetAt(emoji_set, position) {
    return new Promise((resolve, reject) =>
      this.#fn.emoji_set_get_at.async(
        emoji_set,
        position,
        this.error,
        this.checkAsyncRes(resolve, reject, "emojiSetGetAt")
      )
    );
  }

  static emojiSetGetLength(emoji_set) {
    return new Promise((resolve, reject) =>
      this.#fn.emoji_set_get_length.async(
        emoji_set,
        this.error,
        this.checkAsyncRes(resolve, reject, "emojiSetGetLength")
      )
    );
  }
}
module.exports = WalletFFI;
