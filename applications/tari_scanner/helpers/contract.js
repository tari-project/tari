// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

class Contract {
  constructor(id) {
    this.id = id;
    this.committee = [];
    this.creation_height = 0;
    this.name = "";
    this.issuer = "";
    this.committee_size = 0;
    this.checkpoints = [];
    this.last_checkpoint = undefined;
  }

  addToCommittee(member) {
    if (!this.committee.includes(member)) {
      this.committee.push(member);
      ++this.committee_size;
    }
  }

  getAllValidatorNodesIds() {
    return this.committee;
  }

  setCreationHeight(height) {
    this.creation_height = height;
  }

  getCreationHeight() {
    return this.creation_height;
  }

  addCheckpoint(height, validators) {
    this.checkpoints.push({ height, validators });
    if (this.last_checkpoint === undefined || this.last_checkpoint < height) {
      this.last_checkpoint = height;
    }
  }
}

module.exports = {
  Contract,
};
