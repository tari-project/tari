const lib = require("./lib");
const ref = require("ref-napi");
const ffi = require("ffi-napi");

const i32 = ref.types.int32;
const u8 = ref.types.uint8;
const u64 = ref.types.uint64;
const bool = ref.types.bool;

try {
  let seeds = [];
  if (!process.env.SEED_WORDS) {
    console.error(
      "Set your SEED_WORDS env var to your list of seed words separated by single spaces. eg:"
    );
    console.error('SEED_WORDS="one two three ..." npm run recovery');
    process.exit();
  } else {
    seeds = process.env.SEED_WORDS.split(" ");
  }

  if (seeds.length !== 24) {
    console.error(
      `Wrong number of seed words: expected 24, got ${seeds.length}.`
    );
    process.exit();
  }

  let err = ref.alloc(i32);
  // console.log(err);

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
    "./recovery",
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
  // callback_utxo_validation_complete: unsafe extern "C" fn(u64, u8),
  const utxoValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("utxoValidation: ", i, j);
  });
  // callback_stxo_validation_complete: unsafe extern "C" fn(u64, u8),
  const stxoValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("stxoValidation: ", i, j);
  });
  // callback_invalid_txo_validation_complete: unsafe extern "C" fn(u64, u8),
  const itxoValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("itxoValidation: ", i, j);
  });
  // callback_transaction_validation_complete: unsafe extern "C" fn(u64, u8),
  const txValidation = ffi.Callback("void", [u64, u8], function (i, j) {
    console.log("txValidation: ", i, j);
  });
  // callback_saf_messages_received: unsafe extern "C" fn(),
  const safsReceived = ffi.Callback("void", [], function () {
    console.log("safsReceived");
  });

  const recovery = ffi.Callback("void", [u64, u64], function (current, total) {
    console.log("recovery scanning UTXOs: ", { current }, { total });
    getBalance();
    if (current == total) {
      process.exit();
    }
  });

  const seedWords = lib.seed_words_create();

  for (const word of seeds) {
    // console.log(word);
    let pushResult = lib.seed_words_push_word(seedWords, word, err);
    // console.log("r", pushResult);
    // console.log("err", err);
  }

  console.log("Create Wallet from seed words...");
  let wallet = lib.wallet_create(
    comms,
    "./recovery/logs/wallet.log",
    5,
    10240,
    ref.NULL, // passphrase
    seedWords, // seed words
    receivedTx,
    receivedTxReply,
    receivedFinalized,
    txBroadcast,
    txMined,
    txMinedUnconfirmed,
    directSendResult,
    safResult,
    txCancelled,
    utxoValidation,
    stxoValidation,
    itxoValidation,
    txValidation,
    safsReceived,
    err
  );

  getBalance();

  const u8ArrayFromHex = (hexString) =>
    new Uint8Array(
      hexString.match(/.{1,2}/g).map((byte) => parseInt(byte, 16))
    );
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

  console.log("Starting recovery...");
  const temp = lib.wallet_start_recovery(wallet, publicKey, recovery, err);
  console.log("started", temp, err.deref());

  process.stdin.resume();

  function exitHandler(options, exitCode) {
    try {
      console.log("exitHandler");
      console.log("options", options);
      console.log("exitCode", exitCode);
      if (options.cleanup) {
        getBalance();
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
} catch (e) {
  console.error("ERROR: ", e);
}
