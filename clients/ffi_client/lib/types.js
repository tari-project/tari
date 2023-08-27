// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

const ref = require("ref-napi");
const ArrayType = require("ref-array-napi");

const strPtr = ref.refType(ref.types.CString);
const errPtr = ref.refType(ref.types.int32);
const transportRef = ref.refType(ref.types.void);
const commsConfigRef = ref.refType(ref.types.void);
const walletRef = ref.refType(ref.types.void);
const fn = ref.refType(ref.types.void);
const bool = ref.types.bool;
const u8 = ref.types.uint8;
const u16 = ref.types.uint16;
const i32 = ref.types.int32;
const u32 = ref.types.uint32;
const u64 = ref.types.uint64;
const u8Array = ArrayType(ref.types.uint8);
const u8ArrayPtr = ref.refType(u8Array);
const byteVectorRef = ref.refType(u8Array);
const publicKeyRef = ref.refType(ref.types.void);
const publicKeyArrPtr = ref.refType(u8Array);
const strArray = ref.refType(ArrayType(ref.types.void));
const strArrayPtr = ref.refType(ArrayType("string"));

module.exports = {
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
  publicKeyArrPtr,
  strArray,
  strArrayPtr,
};
