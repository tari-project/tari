// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const InterfaceFFI = require("./ffiInterface");
const ByteVector = require("./byteVector");

class CommitmentSignature {
  ptr;
  pointerAssign(ptr) {
    // Prevent pointer from being leaked in case of re-assignment
    if (this.ptr) {
      this.ptr = ptr;
      this.destroy();
    } else {
      this.ptr = ptr;
    }
  }

  getPtr() {
    return this.ptr;
  }

  static createFromObject(obj) {
    const public_nonce_commitment = obj.public_nonce_commitment.toString();
    const signature_u = obj.signature_u.toString();
    const signature_v = obj.signature_v.toString();

    return CommitmentSignature.createFromHex(public_nonce_commitment, signature_u, signature_v);
  }

  static createFromHex(public_nonce_hex, u_hex, v_hex) {
    let public_nonce_bytes = ByteVector.fromBytes(public_nonce_hex.toString("hex")).getPtr();
    let u_bytes = ByteVector.fromBytes(u_hex.toString("hex")).getPtr();
    let v_bytes = ByteVector.fromBytes(v_hex.toString("hex")).getPtr();

    let result = new CommitmentSignature();
    result.pointerAssign(InterfaceFFI.commitmentSignatureCreateFromBytes(
      public_nonce_bytes,
      u_bytes,
      v_bytes
    ));
    return result;
  }

  destroy() {
    if (this.ptr) {
      InterfaceFFI.commitmentSignatureDestroy(this.ptr);
      this.ptr = undefined; //prevent double free segfault
    }
  }
}

module.exports = CommitmentSignature;
