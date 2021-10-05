const ffi = require("ffi-napi");

const {
  strPtr,
  errPtr,
  transportRef,
  commsConfigRef,
  walletRef,
  fn,
  bool,
  u8,
  u16,
  i32,
  u32,
  u64,
  u8Array,
  u8ArrayPtr,
  byteVectorRef,
  publicKeyRef,
  strArray,
  strArrayPtr,
} = require("./types");

// todo: check if the lib exists first

console.log("Set up library...");
const libWallet = ffi.Library("./libtari_wallet_ffi.dylib", {
  byte_vector_create: [byteVectorRef, [u8ArrayPtr, u32, errPtr]],
  comms_config_create: [
    commsConfigRef,
    ["string", transportRef, "string", "string", u64, u64, errPtr],
  ],
  public_key_create: [publicKeyRef, [byteVectorRef, errPtr]],
  public_key_get_bytes: [u8ArrayPtr, [publicKeyRef, errPtr]],
  seed_words_create: [strPtr, []],
  seed_words_get_at: ["string", [strArrayPtr, u32, errPtr]],
  seed_words_push_word: [u8, [strPtr, "string", errPtr]],
  transport_tor_create: [
    transportRef,
    ["string", u8ArrayPtr, u16, "string", "string", errPtr],
  ],
  wallet_add_base_node_peer: [bool, [walletRef, u8ArrayPtr, "string", errPtr]],
  wallet_create: [
    walletRef,
    [
      commsConfigRef,
      "string",
      u32,
      u32,
      "string",
      "string",
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      fn,
      errPtr,
    ],
  ],
  wallet_destroy: ["void", [walletRef]],
  wallet_get_available_balance: [u64, [walletRef, errPtr]],
  wallet_get_pending_incoming_balance: [u64, [walletRef, errPtr]],
  wallet_get_public_key: [publicKeyRef, [walletRef, errPtr]],
  wallet_get_seed_words: [strArrayPtr, [walletRef, errPtr]],
  wallet_get_num_confirmations_required: [u64, [walletRef, errPtr]],
  wallet_set_num_confirmations_required: ["void", [walletRef, u64, errPtr]],
  wallet_start_transaction_validation: [u64, [walletRef, errPtr]],
  wallet_start_txo_validation: [u64, [walletRef, errPtr]],
  wallet_start_recovery: [bool, [walletRef, publicKeyRef, fn, errPtr]],
});

module.exports = libWallet;
