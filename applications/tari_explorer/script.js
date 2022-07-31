// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const OP_CHECK_HEIGHT_VERIFY = 0x66;
const OP_CHECK_HEIGHT = 0x67;
const OP_COMPARE_HEIGHT_VERIFY = 0x68;
const OP_COMPARE_HEIGHT = 0x69;

// Opcode constants: Stack Manipulation
const OP_DROP = 0x70;
const OP_DUP = 0x71;
const OP_REV_ROT = 0x72;
const OP_PUSH_HASH = 0x7a;
const OP_PUSH_ZERO = 0x7b;
const OP_NOP = 0x73;
const OP_PUSH_ONE = 0x7c;
const OP_PUSH_INT = 0x7d;
const OP_PUSH_PUBKEY = 0x7e;

// Opcode constants: Math Operations
const OP_EQUAL = 0x80;
const OP_EQUAL_VERIFY = 0x81;
const OP_ADD = 0x93;
const OP_SUB = 0x94;
const OP_GE_ZERO = 0x82;
const OP_GT_ZERO = 0x83;
const OP_LE_ZERO = 0x84;
const OP_LT_ZERO = 0x85;

// Opcode constants: Boolean Logic
const OP_OR_VERIFY = 0x64;
const OP_OR = 0x65;

// Opcode constants: Cryptographic Operations
const OP_CHECK_SIG = 0xac;
const OP_CHECK_SIG_VERIFY = 0xad;
const OP_CHECK_MULTI_SIG = 0xae;
const OP_CHECK_MULTI_SIG_VERIFY = 0xaf;
const OP_HASH_BLAKE256 = 0xb0;
const OP_HASH_SHA256 = 0xb1;
const OP_HASH_SHA3 = 0xb2;

// Opcode constants: Miscellaneous
const OP_RETURN = 0x60;
const OP_IF_THEN = 0x61;
const OP_ELSE = 0x62;
const OP_END_IF = 0x63;

function u64(data) {
  let n = BigInt(0);
  for (let i = 7; i >= 0; --i) {
    n <<= BigInt(8);
    n |= BigInt(data[i]);
  }
  return n.toString();
}

function i64(data) {
  let n = BigInt(data[7] & 127);
  for (let i = 6; i >= 0; --i) {
    n <<= BigInt(8);
    n |= BigInt(data[i]);
  }
  // It's negative
  if (data[7] & 128) {
    n -= BigInt(9223372036854775807);
  }
  return n.toString();
}

function hex(data) {
  return data ? Buffer.from(data).toString("hex") : "";
}

function script(data) {
  data = [...data];
  let i = 0;
  let s = [];
  let m, n, msg, public_keys;
  while (i < data.length) {
    switch (data[i]) {
      // Opcode constants: Block Height Checks
      case OP_CHECK_HEIGHT_VERIFY:
        s.push(`CHECK_HEIGHT_VERIFY ${u64(data.slice(i + 1, i + 9))}`);
        break;
      case OP_CHECK_HEIGHT:
        s.push(`CHECK_HEIGHT ${u64(data.slice(i + 1, i + 9))}`);
        break;
      case OP_COMPARE_HEIGHT_VERIFY:
        s.push(`COMPARE_HEIGHT_VERIFY`);
        i += 1;
        break;
      case OP_COMPARE_HEIGHT:
        s.push(`COMPARE_HEIGHT`);
        i += 1;
        break;

      // Opcode constants: Stack Manipulation
      case OP_DROP:
        s.push(`DROP`);
        i += 1;
        break;
      case OP_DUP:
        s.push("DUP");
        i += 1;
        break;
      case OP_REV_ROT:
        s.push("REV_ROT");
        i += 1;
        break;
      case OP_PUSH_HASH:
        s.push(`PUSH_HASH ${hex(data.slice(i + 1, i + 33))}`);
        i += 33;
        break;
      case OP_PUSH_ZERO:
        s.push("PUSH_ZERO");
        i += 1;
        break;
      case OP_NOP:
        s.push("NOP");
        i += 1;
        break;
      case OP_PUSH_ONE:
        s.push("PUSH_ONE");
        i += 1;
        break;
      case OP_PUSH_INT:
        s.push(`PUSH_INT ${i64(data.slice(i + 1, i + 9))}`);
        i += 9;
        break;
      case OP_PUSH_PUBKEY:
        s.push(`PUSH_PUBKEY ${hex(data.slice(i + 1, i + 33))}`);
        i += 33;
        break;

      // Opcode constants: Math Operations
      case OP_EQUAL:
        s.push(`EQUAL`);
        i += 1;
        break;
      case OP_EQUAL_VERIFY:
        s.push(`EQUAL_VERIFY`);
        i += 1;
        break;
      case OP_ADD:
        s.push(`ADD`);
        i += 1;
        break;
      case OP_SUB:
        s.push(`SUB`);
        i += 1;
        break;
      case OP_GE_ZERO:
        s.push(`GE_ZERO`);
        i += 1;
        break;
      case OP_GT_ZERO:
        s.push(`GT_ZERO`);
        i += 1;
        break;
      case OP_LE_ZERO:
        s.push(`LE_ZERO`);
        i += 1;
        break;
      case OP_LT_ZERO:
        s.push(`LT_ZERO`);
        i += 1;
        break;

      // Opcode constants: Boolean Logic
      case OP_OR_VERIFY:
        s.push(`OR_VERIFY ${data[i + 1]}`);
        i += 2;
        break;
      case OP_OR:
        s.push(`OR ${data[i + 1]}`);
        i += 2;
        break;

      // Opcode constants: Cryptographic Operations
      case OP_CHECK_SIG:
        s.push(`CHECK_SIG ${hex(data.slice(i + 1, i + 33))}`);
        i += 33;
        break;
      case OP_CHECK_SIG_VERIFY:
        s.push(`CHECK_SIG_VERIFY ${hex(data.slice(i + 1, i + 33))}`);
        i += 33;
        break;
      case OP_CHECK_MULTI_SIG:
        m = data[i + 1];
        n = data[i + 2];
        public_keys = [];
        i += 3;
        for (let j = 0; j < n; ++j) {
          public_keys = hex(data.slice(i, i + 32));
          i += 32;
        }
        msg = data.slice(i, i + 32);
        i += 32;
        s.push(`CHECK_MULTI_SIG ${m} ${n} [${public_keys.join(",")}] ${msg}`);
        break;
      case OP_CHECK_MULTI_SIG_VERIFY:
        m = data[i + 1];
        n = data[i + 2];
        public_keys = [];
        i += 3;
        for (let j = 0; j < n; ++j) {
          public_keys = hex(data.slice(i, i + 32));
          i += 32;
        }
        msg = data.slice(i, i + 32);
        i += 32;
        s.push(
          `CHECK_MULTI_SIG_VERIFY ${m} ${n} [${public_keys.join(",")}] ${msg}`
        );
        break;
      case OP_HASH_BLAKE256:
        s.push(`HASH_BLAKE256`);
        i += 1;
        break;
      case OP_HASH_SHA256:
        s.push(`HASH_SHA256`);
        i += 1;
        break;
      case OP_HASH_SHA3:
        s.push(`HASH_SHA3`);
        i += 1;
        break;

      // Opcode constants: Miscellaneous
      case OP_RETURN:
        s.push(`RETURN`);
        i += 1;
        break;
      case OP_IF_THEN:
        s.push(`IF_THEN`);
        i += 1;
        break;
      case OP_ELSE:
        s.push(`ELSE`);
        i += 1;
        break;
      case OP_END_IF:
        s.push(`END_IF`);
        i += 1;
        break;
      default:
        s.push("UNKNOWN");
        i += 1;
        break;
    }
  }
  return s.join("<br/>");
}

module.exports = [hex, script];
