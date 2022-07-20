// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { Contract } = require("./contract");

class Contracts {
  constructor() {
    this.contracts = {};
  }

  addContract(id) {
    if (!(id in this.contracts)) {
      this.contracts[id] = new Contract(id);
    }
    return this.contracts[id];
  }

  getContract(id) {
    return this.contracts?.[id];
  }

  getAllIDs() {
    return Object.values(this.contracts);
  }
}

let contracts = new Contracts();
console.log("contracts creation");

module.exports = {
  contracts,
};
