const net = require("net");

const { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");
const { expect } = require("chai");

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

async function tryConnect(makeClient, opts = {}) {
  const options = Object.assign(
    {
      deadline: Infinity,
      maxAttempts: 3,
    },
    opts
  );
  let attempts = 0;
  for (;;) {
    let client = makeClient();

    // Don't log the uninteresting case
    if (attempts > 0) {
      console.warn(
        `GRPC connection attempt ${attempts + 1}/${options.maxAttempts}`
      );
    }
    let error = await new Promise((resolve) => {
      client.waitForReady(options.deadline, (err) => {
        if (err) {
          return resolve(err);
        }
        resolve(null);
      });
    });

    if (error) {
      if (attempts >= options.maxAttempts) {
        throw error;
      }
      attempts++;
      console.error(
        `Failed connection attempt ${attempts + 1}/${options.maxAttempts}`
      );
      console.error(error);
      await sleep(1000);
      continue;
    }

    return client;
  }
}

async function waitFor(
  asyncTestFn,
  toBe,
  maxTimeMs,
  timeOut = 500,
  skipLog = 50
) {
  const now = new Date();

  let i = 0;
  while (new Date() - now < maxTimeMs) {
    try {
      const value = await asyncTestFn();
      if (value === toBe) {
        if (i > 1) {
          console.log("waiting for process...", timeOut, i, value);
        }
        return true;
      }
      if (i % skipLog === 0 && i > 1) {
        console.log("waiting for process...", timeOut, i, value);
      }
      await sleep(timeOut);
      i++;
    } catch (e) {
      if (i > 1) {
        if (e && e.code && e.code === NO_CONNECTION) {
          // console.log("No connection yet (waitFor)...");
        } else {
          console.error("Error in waitFor: ", e);
        }
      }
      await sleep(timeOut);
    }
  }
  return false;
}

async function waitForIterate(testFn, toBe, sleepMs, maxIterations = 500) {
  let count = 0;
  let val = await Promise.resolve(testFn());
  while (val !== toBe) {
    val = await Promise.resolve(testFn());
    if (count >= maxIterations) {
      break;
    }
    count++;
    await sleep(sleepMs);
    process.stdout.write(".");
  }
  return val;
}

async function waitForPredicate(predicate, timeOut, sleepMs = 500) {
  let elapsed = 0;
  while (elapsed < timeOut) {
    const val = await predicate();
    if (val) {
      return val;
    }
    await sleep(sleepMs);
    elapsed += sleepMs;
    process.stdout.write(".");
  }
  throw new Error(`Predicate was not truthy after ${timeOut} ms`);
}

function dec2hex(n) {
  return n ? [n % 256].concat(dec2hex(~~(n / 256))) : [];
}

function toLittleEndianInner(n) {
  let hexar = dec2hex(n);
  hexar = hexar.map((h) => (h < 16 ? "0" : "") + h.toString(16));
  if (hexar.length < 4) {
    return hexar.concat(Array(4 - hexar.length).fill("00"));
  } else {
    return hexar;
  }
}

function toLittleEndian(n, numBits) {
  const s = toLittleEndianInner(n);

  for (let i = s.length; i < numBits / 8; i++) {
    s.push("00");
  }

  const arr = Buffer.from(s.join(""), "hex");

  return arr;
}

function littleEndianHexStringToBigEndianHexString(string) {
  if (!string) return undefined;
  var len = string.length;
  var bigEndianHexString = "0x";
  for (var i = 0; i < len / 2; i++) {
    bigEndianHexString += string.substring(len - (i + 1) * 2, len - i * 2);
  }
  return bigEndianHexString;
}

function hexSwitchEndianness(val) {
  let res = "";
  for (let i = val.length - 2; i > 0; i -= 2) {
    res += val[i] + val[i + 1];
  }
  return res;
}

const getFreePort = function () {
  return new Promise((resolve, reject) => {
    const srv = net.createServer(function (_sock) {});
    srv.listen(0, function () {
      let { port } = srv.address();
      srv.close((err) => {
        if (err) {
          reject(err);
        } else {
          resolve(port);
        }
      });
    });
    srv.on("error", function (err) {
      reject(err);
    });
  });
};

const encodeOption = function (value) {
  let buffer;
  if (value) {
    buffer = Buffer.concat([Buffer.from([1]), Buffer.from(value, "utf8")]);
  } else {
    buffer = Buffer.from([0]);
  }
  return buffer;
};

const getTransactionOutputHash = function (output) {
  const KEY = null; // optional key
  const OUTPUT_LENGTH = 32; // bytes
  const context = blake2bInit(OUTPUT_LENGTH, KEY);
  let encodedBytesLength = 0;
  // version
  const version = Buffer.from([0]);
  encodedBytesLength += version.length;
  blake2bUpdate(context, version);
  // features
  let features = Buffer.concat([
    // features.version
    Buffer.from([0]),
    // features.maturity
    Buffer.from([parseInt(output.features.maturity)]),
    // features.flags
    Buffer.from([output.features.flags]),
  ]);
  // features.parent_public_key
  features = Buffer.concat([
    Buffer.from(features),
    encodeOption(output.features.parent_public_key),
  ]);
  // features.unique_id
  features = Buffer.concat([
    Buffer.from(features),
    encodeOption(output.features.unique_id),
  ]);
  // features.asset
  features = Buffer.concat([
    Buffer.from(features),
    encodeOption(output.features.asset),
  ]);
  // features.mint_non_fungible
  features = Buffer.concat([
    Buffer.from(features),
    encodeOption(output.features.mint_non_fungible),
  ]);
  // features.sidechain_checkpoint
  features = Buffer.concat([
    Buffer.from(features),
    encodeOption(output.features.sidechain_checkpoint),
  ]);
  // features.metadata
  features = Buffer.concat([
    Buffer.from(features),
    Buffer.from([output.features.metadata.length]),
    Buffer.from(output.features.metadata),
  ]);
  encodedBytesLength += features.length;
  blake2bUpdate(context, features);
  // commitment
  encodedBytesLength += output.commitment.length;
  blake2bUpdate(context, output.commitment);
  // script
  const script = Buffer.concat([
    Buffer.from([output.script.length]),
    Buffer.from(output.script),
  ]);
  encodedBytesLength += script.length;
  blake2bUpdate(context, script);
  // covenant
  const covenant = Buffer.concat([
    Buffer.from([output.covenant.length]),
    Buffer.from(output.covenant),
  ]);
  encodedBytesLength += covenant.length;
  blake2bUpdate(context, covenant);

  expect(context.c).to.equal(encodedBytesLength);
  const hash = blake2bFinal(context);
  const hashBuffer = Buffer.from(hash);
  // console.log(
  //   "\ngetTransactionOutputHash - hash",
  //   hashBuffer.toString("hex"),
  //   "\n"
  // );
  return hashBuffer;
};

function consoleLogTransactionDetails(txnDetails) {
  console.log(
    "  Transaction " +
      pad("'" + txnDetails.tx_id + "'", 24) +
      " has status " +
      pad("'" + txnDetails.status + "'", 40) +
      " and " +
      pad("is_cancelled(" + txnDetails.is_cancelled + ")", 21)
  );
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

function pad(str, length, padLeft = true) {
  const padding = Array(length).join(" ");
  if (typeof str === "undefined") return padding;
  if (padLeft) {
    return (padding + str).slice(-padding.length);
  } else {
    return (str + padding).substring(" ", padding.length);
  }
}

function combineTwoTariKeys(key1, key2) {
  let total_key =
    BigInt(littleEndianHexStringToBigEndianHexString(key1)) +
    BigInt(littleEndianHexStringToBigEndianHexString(key2));
  if (total_key < 0) {
    total_key =
      total_key +
      BigInt(
        littleEndianHexStringToBigEndianHexString(
          "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010"
        )
      );
  }
  total_key = total_key.toString(16);
  while (total_key.length < 64) {
    total_key = "0" + total_key;
  }
  total_key = littleEndianHexStringToBigEndianHexString(total_key);
  while (total_key.length < 64) {
    total_key = "0" + total_key;
  }
  return total_key;
}

const multiAddrToSocket = (string) => {
  console.log("input:", string);
  let match = string.match(/\/ip4\/(.*)\/tcp\/(.*)/);
  if (!match) {
    console.log("no match, returning as is");
    return string;
  }
  let res = `${match[1]}:${match[2]}`;
  console.log("returning: ", res);
  return res;
};

const byteArrayToHex = (bytes) =>
  bytes.reduce((str, byte) => str + byte.toString(16).padStart(2, "0"), "");

const convertHexStringToVec = (string) =>
  string.match(/.{2}/g).map((x) => parseInt(x, 16));

const convertStringToVec = (string) =>
  Array(string.length)
    .fill()
    .map((_, i) => string.charCodeAt(i));

module.exports = {
  getRandomInt,
  sleep,
  waitFor,
  toLittleEndian,
  littleEndianHexStringToBigEndianHexString,
  // portInUse,
  getFreePort,
  getTransactionOutputHash,
  hexSwitchEndianness,
  consoleLogTransactionDetails,
  tryConnect,
  consoleLogBalance,
  withTimeout,
  combineTwoTariKeys,
  byteArrayToHex,
  convertHexStringToVec,
  convertStringToVec,
  waitForPredicate,
  waitForIterate,
  NO_CONNECTION,
  multiAddrToSocket,
};
