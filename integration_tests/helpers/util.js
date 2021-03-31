var net = require("net");
const fs = require("fs");
const readline = require("readline");

var { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");

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

async function waitFor(
  asyncTestFn,
  toBe,
  maxTime,
  timeOut = 500,
  skipLog = 50
) {
  var now = new Date();

  let i = 0;
  while (new Date() - now < maxTime) {
    const value = await asyncTestFn();
    if (value === toBe) {
      if (i > 1) {
        console.log("waiting for process...", timeOut, i);
      }
      break;
    }
    if (i % skipLog == 0 && i > 1) {
      console.log("waiting for process...", timeOut, i);
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
var portInUse = function (port, callback) {
  var server = net.createServer(function (socket) {
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

var index = 0;
var getFreePort = async function (from, to) {
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

// WIP  this doesn't hash properly
const getTransactionOutputHash = function (output) {
  var KEY = null; // optional key
  var OUTPUT_LENGTH = 32; // bytes
  var context = blake2bInit(OUTPUT_LENGTH, KEY);
  let flags = Buffer.alloc(1);
  flags[0] = output.features.flags;
  var buffer = Buffer.concat([
    flags,
    toLittleEndian(parseInt(output.features.maturity), 64),
  ]);
  blake2bUpdate(context, buffer);
  blake2bUpdate(context, output.commitment);
  let final = blake2bFinal(context);
  return Buffer.from(final);
};

function consoleLogTransactionDetails(txnDetails, txId) {
  var found = txnDetails[0];
  var status = txnDetails[1];
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
  var padding = Array(length).join(" ");
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
};
