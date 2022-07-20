// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

class ValidatorNode {
  constructor(id) {
    this.id = id;
    this.contracts = {};
    this.first_signature = undefined;
    this.checkpoints = 0;
    this.missed_checkpoints = 0;
    this.last_checkpoint = {};
  }

  addContract(contract_id, height) {
    if (!this.contracts[contract_id]) {
      this.contracts[contract_id] = new Set();
    }
    if (height !== undefined) {
      this.first_signature = this.first_signature === undefined ? height : Math.min(this.first_signature, height);
    }
  }

  getAllContractsIds() {
    return Object.keys(this.contracts);
  }

  addCheckpoint(contract_id, height) {
    if (!this.contracts[contract_id]) {
      this.contracts[contract_id] = new Set();
    }
    this.contracts[contract_id].add(height);
    if (this.last_checkpoint[contract_id] === undefined || this.last_checkpoint[contract_id] < height)
      this.last_checkpoint[contract_id] = height;
  }
}

module.exports = {
  ValidatorNode,
};
