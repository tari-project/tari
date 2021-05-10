const net = require("net");

const { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");

const NO_CONNECTION = 14;

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
  const now = new Date();

  let i = 0;
  while (new Date() - now < maxTime) {
    try {
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
    } catch (e) {
      if (e && e.code && e.code === NO_CONNECTION) {
        console.log("No connection yet (waitFor)...");
      } else {
        console.error("Error in waitFor: ", e);
      }
    }
  }
}

function dec2hex(n) {
  return n ? [n % 256].concat(dec2hex(~~(n / 256))) : [];
}

function toLittleEndianInner(n) {
  const hexar = dec2hex(n);
  return hexar
    .map((h) => (h < 16 ? "0" : "") + h.toString(16))
    .concat(Array(4 - hexar.length).fill("00"));
}

function toLittleEndian(n, numBits) {
  const s = toLittleEndianInner(n);

  for (let i = s.length; i < numBits / 8; i++) {
    s.push("00");
  }

  const arr = Buffer.from(s.join(""), "hex");

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
const portInUse = function (port, callback) {
  const server = net.createServer(function (socket) {
    socket.write("Echo server\r\n");
    socket.pipe(socket);
  });

  server.listen(port, "127.0.0.1");
  server.on("error", function () {
    callback(true);
  });
  server.on("listening", function () {
    server.close();
    callback(false);
  });
};

let index = 0;
const getFreePort = async function (from, to) {
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
    const notInUse = await testPort(port);
    // console.log("Port not in use:", notInUse);
    if (notInUse) {
      return port;
    }
  }
};

// WIP  this doesn't hash properly
const getTransactionOutputHash = function (output) {
  const KEY = null; // optional key
  const OUTPUT_LENGTH = 32; // bytes
  const context = blake2bInit(OUTPUT_LENGTH, KEY);
  const flags = Buffer.alloc(1);
  flags[0] = output.features.flags;
  const buffer = Buffer.concat([
    flags,
    toLittleEndian(parseInt(output.features.maturity), 64),
  ]);
  blake2bUpdate(context, buffer);
  blake2bUpdate(context, output.commitment);
  const final = blake2bFinal(context);
  return Buffer.from(final);
};

function consoleLogTransactionDetails(txnDetails, txId) {
  const found = txnDetails[0];
  const status = txnDetails[1];
  if (found) {
    console.log(
      "  Transaction " +
        pad("'" + status.transactions[0].tx_id + "'", 24) +
        " has status " +
        pad("'" + status.transactions[0].status + "'", 40) +
        " and " +
        pad("is_cancelled(" + status.transactions[0].is_cancelled + ")", 21) +
        " and " +
        pad("is_valid(" + status.transactions[0].valid + ")", 16)
    );
  } else {
    console.log("  Transaction '" + txId + "' " + status);
  }
}

function consoleLogBalance(balance) {
  console.log(
    "  Available " +
      pad(balance.available_balance, 16) +
      " uT, Pending incoming " +
      pad(balance.pending_incoming_balance, 16) +
      " uT, Pending outgoing " +
      pad(balance.pending_outgoing_balance, 16) +
      " uT"
  );
}

function consoleLogCoinbaseDetails(txnDetails) {
  console.log(
    "  Transaction " +
      pad("'" + txnDetails.tx_id + "'", 24) +
      " has status " +
      pad("'" + txnDetails.status + "'", 40) +
      " and " +
      pad("is_cancelled(" + txnDetails.is_cancelled + ")", 21) +
      " and " +
      pad("is_valid(" + txnDetails.valid + ")", 16)
  );
}

function pad(str, length, padLeft = true) {
  const padding = Array(length).join(" ");
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
  NO_CONNECTION,
};
