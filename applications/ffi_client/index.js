// this is nasty
//  ¯\_(ツ)_/¯

// TODO: Use implementation in cucumber tests instead (see helpers/ffi).

const lib = require("./lib");
const ref = require("ref-napi");
const ffi = require("ffi-napi");

const i32 = ref.types.int32;
const u8 = ref.types.uint8;
const u64 = ref.types.uint64;
const bool = ref.types.bool;

try {
  let err = ref.alloc(i32);
  // console.log(err);

  let recoveryInProgress = ref.alloc(bool);

  console.log("Create Tor transport...");
  let tor = lib.transport_tor_create(
    "/ip4/127.0.0.1/tcp/9051",
    ref.NULL,
    9051,
    ref.NULL,
    ref.NULL,
    err
  );

  // todo: error handling

  console.log("Create Comms config...");
  let comms = lib.comms_config_create(
    "/ip4/0.0.0.0/tcp/9838",
    tor,
    "wallet.dat",
    "./wallet",
    30,
    600,
    err
  );

  // callback_received_transaction: unsafe extern "C" fn(*mut TariPendingInboundTransaction),
  const receivedTx = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("receivedTx: ", ptr);
  });
  // callback_received_transaction_reply: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const receivedTxReply = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("receivedTxReply: ", ptr);
  });
  // callback_received_finalized_transaction: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const receivedFinalized = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("receivedFinalized: ", ptr);
  });
  // callback_transaction_broadcast: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const txBroadcast = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("txBroadcast: ", ptr);
  });
  // callback_transaction_mined: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const txMined = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("txMined: ", ptr);
  });
  // callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut TariCompletedTransaction, u64),
  const txMinedUnconfirmed = ffi.Callback(
    "void",
    ["pointer"],
    function (ptr, confirmations) {
      console.log("txMinedUnconfirmed: ", ptr, confirmations);
    }
  );
  // callback_faux_transaction_confirmed: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const txFauxConfirmed = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("txFauxConfirmed: ", ptr);
  });
  // callback_faux_transaction_unconfirmed: unsafe extern "C" fn(*mut TariCompletedTransaction, u64),
  const txFauxUnconfirmed = ffi.Callback(
      "void",
      ["pointer"],
      function (ptr, confirmations) {
        console.log("txFauxUnconfirmed: ", ptr, confirmations);
      }
  );
  // callback_direct_send_result: unsafe extern "C" fn(c_ulonglong, bool),
  const directSendResult = ffi.Callback("void", [u64, bool], function (i, j) {
    console.log("directSendResult: ", i, j);
  });
  // callback_store_and_forward_send_result: unsafe extern "C" fn(c_ulonglong, bool),
  const safResult = ffi.Callback("void", [u64, bool], function (i, j) {
    console.log("safResult: ", i, j);
  });
  // callback_transaction_cancellation: unsafe extern "C" fn(*mut TariCompletedTransaction),
  const txCancelled = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("txCancelled: ", ptr);
  });
  // callback_txo_validation_complete: unsafe extern "C" fn(u64, u8),
  const txoValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("utxoValidation: ", i, j);
  });
  // callback_balance_updated: unsafe extern "C" fn(*mut Balance),
  const balanceUpdated = ffi.Callback("void", ["pointer"], function (ptr) {
    console.log("balanceUpdated: ", ptr);
  });
  // callback_transaction_validation_complete: unsafe extern "C" fn(u64, u8),
  const txValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("txValidation: ", i, j);
  });
  // callback_saf_messages_received: unsafe extern "C" fn(),
  const safsReceived = ffi.Callback("void", [], function () {
    console.log("safsReceived");
  });

  console.log("Create Wallet...");
  let wallet = lib.wallet_create(
    comms,
    "./wallet/logs/wallet.log",
    5,
    10240,
    ref.NULL, // passphrase
    ref.NULL, // seed words
    receivedTx,
    receivedTxReply,
    receivedFinalized,
    txBroadcast,
    txMined,
    txMinedUnconfirmed,
    txFauxConfirmed,
    txFauxUnconfirmed,
    directSendResult,
    safResult,
    txCancelled,
    txoValidation,
    balanceUpdated,
    txValidation,
    safsReceived,
    recoveryInProgress,
    err
  );

  // look ma, zero confs!
  lib.wallet_set_num_confirmations_required(wallet, 0, err);
  // console.log(err.deref());
  let confs = lib.wallet_get_num_confirmations_required(wallet, err);
  // console.log("confs", confs);
  // console.log(err.deref());

  let s = lib.wallet_get_seed_words(wallet, err);
  // console.log("seeds words", s);
  // console.log("err", err);
  let seedWords = [];
  for (let i = 0; i < 24; i++) {
    let word = lib.seed_words_get_at(s, i, err);
    // console.log("word", word);
    // console.log("err", err.deref());
    seedWords.push(word);
  }
  console.log("seedWords", seedWords);

  function getBalance() {
    try {
      console.log("===");
      console.log("Balance");
      console.log("===");
      let available = lib.wallet_get_available_balance(wallet, err);
      console.log("  available : ", available);
      let pending_in = lib.wallet_get_pending_incoming_balance(wallet, err);
      console.log("  pending_in: ", pending_in);
      console.log("===");
    } catch (e) {
      console.error("balance error: ", e);
    }
  }
  let j = setInterval(getBalance, 10000);

  const u8ArrayFromHex = (hexString) =>
    new Uint8Array(
      hexString.match(/.{1,2}/g).map((byte) => parseInt(byte, 16))
    );
  const u8ArrayToHex = (bytes) =>
    bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, "0"), "");

  // let myPublicKey = lib.wallet_get_public_key(wallet, err);
  // console.log(myPublicKey);
  // console.log(err);
  // let temp = lib.public_key_get_bytes(myPublicKey, err);
  // console.log("temp", temp.deref());

  // process.exit();

  let publicKeyHex =
    "0c3fe3c23866ed3827e1cd72aae0c9d364d860d597993104e90d9a9401e52f05";
  let publicKeyBytes = u8ArrayFromHex(publicKeyHex);
  let publicKeyByteVector = lib.byte_vector_create(publicKeyBytes, 32, err);
  let publicKey = lib.public_key_create(publicKeyByteVector, err);

  console.log("Set base node peer...", publicKeyHex);
  lib.wallet_add_base_node_peer(
    wallet,
    publicKey,
    "/onion3/2m2xnylrsqbaozsndkbmfisxxbwh2vgvs6oyfak2qah4snnxykrf7zad:18141",
    err
  );

  setTimeout(function () {
    try {
      console.log("start tx validation");
      let id = lib.wallet_start_transaction_validation(wallet, err);
      console.log("tx validation request id", id);

      console.log("start txo validation");
      id = lib.wallet_start_txo_validation(wallet, err);
      console.log("txo validation request id", id);
    } catch (e) {
      console.error("validation error: ", e);
    }
  }, 5000);

  console.log("Wallet running...");
  console.log("Ctrl+C to quit.");
  process.stdin.resume();

  function exitHandler(options, exitCode) {
    try {
      console.log("exitHandler");
      console.log("options", options);
      console.log("exitCode", exitCode);
      if (options.cleanup) {
        console.log("Exiting...");
        lib.wallet_destroy(wallet);
        console.log("Goodbye.");
      }
      if (exitCode || exitCode === 0) console.log("\nExit code:", exitCode);
      if (options.exit) process.exit();
    } catch (e) {
      console.error("exitHandler error", e);
    }
  }

  process.on("exit", exitHandler.bind(null, { cleanup: true, signal: "exit" }));
  process.on(
    "SIGINT",
    exitHandler.bind(null, { exit: true, signal: "SIGINT" })
  );
  process.on(
    "SIGUSR1",
    exitHandler.bind(null, { exit: true, signal: "SIGUSR1" })
  );
  process.on(
    "SIGUSR2",
    exitHandler.bind(null, { exit: true, signal: "SIGUSR2" })
  );
  process.on(
    "uncaughtException",
    exitHandler.bind(null, { exit: true, signal: "uncaughtException" })
  );
} catch (e) {
  console.error("ERROR: ", e);
}
