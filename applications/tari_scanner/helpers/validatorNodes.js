// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { ValidatorNode } = require("./validatorNode");

class ValidatorNodes {
  constructor() {
    this.validator_nodes = {};
  }

  addValidatorNode(id) {
    if (!(id in this.validator_nodes)) {
      this.validator_nodes[id] = new ValidatorNode(id);
    }
    return this.validator_nodes[id];
  }

  getValidatorNode(id) {
    return this.validator_nodes[id];
  }

  getValidatorNode(id) {
    return this.validator_nodes?.[id];
  }

  getAllIDs() {
    return Object.values(this.validator_nodes);
  }
}

let validator_nodes = new ValidatorNodes();

module.exports = {
  validator_nodes,
};
