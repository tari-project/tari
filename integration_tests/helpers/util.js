const net = require("net");
const fs = require("fs");
const readline = require("readline");

const { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");

function getRandomInt(min, max) {
  min = Math.ceil(min);
  max = Math.floor(max);
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

function withTimeout(ms, promise, message = "") {
  const timeout = new Promise((_resolve, reject) => {
    setTimeout(
      () => reject(new Error(message || `Timed out after ${ms}ms`)),
      ms
    );
  });
  return Promise.race([timeout, promise]);
}

async function waitFor(
  asyncTestFn,
  toBe,
  maxTime,
  timeOut = 500,
  skipLog = 50
) {
  let now = new Date();

  let i = 0;
  while (new Date() - now < maxTime) {
    const value = await asyncTestFn();
    if (value === toBe) {
      if (i > 1) {
        console.log("waiting for process...", timeOut, i, value);
      }
      break;
    }
    if (i % skipLog === 0 && i > 1) {
      console.log("waiting for process...", timeOut, i, value);
    }
    await sleep(timeOut);
    i++;
  }
}

function dec2hex(n) {
  return n ? [n % 256].concat(dec2hex(~~(n / 256))) : [];
}

function toLittleEndianInner(n) {
  let hexar = dec2hex(n);
  return hexar
    .map((h) => (h < 16 ? "0" : "") + h.toString(16))
    .concat(Array(4 - hexar.length).fill("00"));
}

function toLittleEndian(n, numBits) {
  let s = toLittleEndianInner(n);

  for (let i = s.length; i < numBits / 8; i++) {
    s.push("00");
  }

  let arr = Buffer.from(s.join(""), "hex");

  return arr;
}

function hexSwitchEndianness(val) {
  let res = "";
  for (let i = val.length - 2; i > 0; i -= 2) {
    res += val[i] + val[i + 1];
  }
  return res;
}

// Thanks to https://stackoverflow.com/questions/29860354/in-nodejs-how-do-i-check-if-a-port-is-listening-or-in-use
let portInUse = function (port, callback) {
  let server = net.createServer(function (socket) {
    socket.write("Echo server\r\n");
    socket.pipe(socket);
  });

  server.listen(port, "127.0.0.1");
  server.on("error", function (e) {
    callback(true);
  });
  server.on("listening", function (e) {
    server.close();
    callback(false);
  });
};

let index = 0;
let getFreePort = async function (from, to) {
  function testPort(port) {
    return new Promise((r) => {
      portInUse(port, (v) => {
        if (v) {
          r(false);
        }
        r(true);
      });
    });
  }

  let port = from + index;
  if (port > to) {
    index = from;
    port = from;
  }
  while (port < to) {
    // let port = getRandomInt(from, to);
    // await sleep(100);
    port++;
    index++;
    let notInUse = await testPort(port);
    // console.log("Port not in use:", notInUse);
    if (notInUse) {
      return port;
    }
  }
};

const getTransactionOutputHash = function (output) {
  let KEY = null; // optional key
  let OUTPUT_LENGTH = 32; // bytes
  let context = blake2bInit(OUTPUT_LENGTH, KEY);
  let flags = Buffer.alloc(1);
  flags[0] = output.features.flags;
  let buffer = Buffer.concat([
    flags,
    toLittleEndian(parseInt(output.features.maturity), 64),
  ]);
  let nop_script_hash =
    "2682c826cae74c92c0620c9ab73c7e577a37870b4ce42465a4e63b58ee4d2408";
  blake2bUpdate(context, buffer);
  blake2bUpdate(context, output.commitment);
  blake2bUpdate(context, Buffer.from(nop_script_hash, "hex"));
  blake2bUpdate(context, output.script_offset_public_key);
  let final = blake2bFinal(context);
  return Buffer.from(final);
};

function calculateBeta(script_hash, features, script_offset_public_key) {
  let KEY = null; // optional key
  let OUTPUT_LENGTH = 32; // bytes
  let context = blake2bInit(OUTPUT_LENGTH, KEY);
  let flags = Buffer.alloc(1);
  flags[0] = features.flags;
  let features_buffer = Buffer.concat([
    flags,
    toLittleEndian(parseInt(features.maturity), 64),
  ]);

  blake2bUpdate(context, Buffer.from(script_hash, "hex"));
  blake2bUpdate(context, features_buffer);
  blake2bUpdate(context, Buffer.from(script_offset_public_key, "hex"));
  let final = blake2bFinal(context);
  return Buffer.from(final);
}

function consoleLogTransactionDetails(txnDetails, txId) {
  let found = txnDetails[0];
  let status = txnDetails[1];
  if (found) {
    console.log(
      "  Transaction " +
        pad("'" + status.transactions[0]["tx_id"] + "'", 24) +
        " has status " +
        pad("'" + status.transactions[0]["status"] + "'", 40) +
        " and " +
        pad(
          "is_cancelled(" + status.transactions[0]["is_cancelled"] + ")",
          21
        ) +
        " and " +
        pad("is_valid(" + status.transactions[0]["valid"] + ")", 16)
    );
  } else {
    console.log("  Transaction '" + txId + "' " + status);
  }
}

function consoleLogBalance(balance) {
  console.log(
    "  Available " +
      pad(balance["available_balance"], 16) +
      " uT, Pending incoming " +
      pad(balance["pending_incoming_balance"], 16) +
      " uT, Pending outgoing " +
      pad(balance["pending_outgoing_balance"], 16) +
      " uT"
  );
}

function consoleLogCoinbaseDetails(txnDetails) {
  console.log(
    "  Transaction " +
      pad("'" + txnDetails["tx_id"] + "'", 24) +
      " has status " +
      pad("'" + txnDetails["status"] + "'", 40) +
      " and " +
      pad("is_cancelled(" + txnDetails["is_cancelled"] + ")", 21) +
      " and " +
      pad("is_valid(" + txnDetails["valid"] + ")", 16)
  );
}

function pad(str, length, padLeft = true) {
  let padding = Array(length).join(" ");
  if (typeof str === "undefined") return padding;
  if (padLeft) {
    return (padding + str).slice(-padding.length);
  } else {
    return (str + padding).substring(" ", padding.length);
  }
}

module.exports = {
  getRandomInt,
  sleep,
  waitFor,
  toLittleEndian,
  // portInUse,
  getFreePort,
  getTransactionOutputHash,
  hexSwitchEndianness,
  consoleLogTransactionDetails,
  consoleLogBalance,
  consoleLogCoinbaseDetails,
  withTimeout,
  calculateBeta,
};
