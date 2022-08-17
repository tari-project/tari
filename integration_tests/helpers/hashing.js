/*
 * // Copyright 2022. The Tari Project
 * //
 * // Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
 * // following conditions are met:
 * //
 * // 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
 * // disclaimer.
 * //
 * // 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
 * // following disclaimer in the documentation and/or other materials provided with the distribution.
 * //
 * // 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
 * // products derived from this software without specific prior written permission.
 * //
 * // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
 * // INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * // DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * // SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * // SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
 * // WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
 * // USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

const { blake2bInit, blake2bUpdate, blake2bFinal } = require("blakejs");
const { toLittleEndian } = require("./util");

class Blake256 {
  constructor() {
    this.hasher = blake2bInit(32);
  }

  chain(value, encoding = undefined) {
    this.update(value, encoding);
    return this;
  }

  update(value, encoding = undefined) {
    let buf = Buffer.isBuffer(value) ? value : Buffer.from(value, encoding);
    blake2bUpdate(this.hasher, buf);
  }

  finalize() {
    return blake2bFinal(this.hasher);
  }
}

class DomainHashing {
  constructor(label) {
    this.hasher = new Blake256();
    this.update(Buffer.from(label, "utf-8"));
  }
  chain(value, encoding = undefined) {
    this.update(value, encoding);
    return this;
  }

  update(value, encoding = undefined) {
    let buf = Buffer.isBuffer(value) ? value : Buffer.from(value, encoding);
    let le = toLittleEndian(buf.length, 64);
    this.hasher.update(le);
    this.hasher.update(buf);
  }

  finalize() {
    return this.hasher.finalize();
  }
}

exports.DomainHashing = DomainHashing;
exports.Blake256 = Blake256;

function hasherWithLabel(label) {
  let len = toLittleEndian(label.length, 64);
  let hasher = new Blake256();
  return hasher.chain(len).chain(label, "utf-8");
}

module.exports = {
  // DomainHashing,
  Blake256,
  domainHashers: {
    transactionKdf(label) {
      return new DomainHashing(
        `com.tari.base_layer.core.transactions.kdf.v0.${label}`
      );
    },
  },
  consensusHashers: {
    transactionHasher(label) {
      return hasherWithLabel(
        `com.tari.base_layer.core.transactions.v0.${label}`
      );
    },
  },
};
