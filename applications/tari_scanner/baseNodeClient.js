// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var { Client } = require("@tari/base-node-grpc-client");
const { contracts } = require("./helpers/contracts");
const { validator_nodes } = require("./helpers/validatorNodes");
var { range } = require("./utils");

class ExpressClient {
  constructor() {
    this.client = Client.connect("localhost:18142");
    this.blocks = {};
    this.validator_nodes = validator_nodes;
    this.contracts = contracts;
  }

  getTip() {
    return this.client.getTipInfo().then((tip_info) => parseInt(tip_info.metadata.height_of_longest_chain));
  }

  #getBlocksArray(from, to) {
    let result = [];
    for (var i = from; i < to; ++i) {
      if (i in this.blocks) {
        result.push(this.blocks[i]);
      }
    }
    return result;
  }

  getBlocks(from, to, refresh = false) {
    let heights = range(from, to);
    if (!refresh) {
      heights = heights.filter((height) => !(height in this.blocks));
    }
    if (heights.length) {
      return this.client.getBlocks({ heights: heights }).then((blocks) => {
        for (const block of blocks) {
          const height = parseInt(block.block.header.height);
          this.blocks[height] = block.block;
        }
        return this.#getBlocksArray(from, to);
      });
    } else {
      return this.#getBlocksArray(from, to);
    }
  }

  #updateFromContractDefinition(sidechain_features, height) {
    let contract_id = sidechain_features.contract_id.toString("hex");
    let contract = contracts.addContract(contract_id, height);
    contract.name = sidechain_features.definition.contract_name.toString();
    contract.issuer = sidechain_features.definition.contract_issuer.toString("hex");
  }

  #updateFromContractConstitution(sidechain_features) {
    let contract_id = sidechain_features.contract_id.toString("hex");
    for (let member of sidechain_features.constitution.validator_committee.members) {
      member = member.toString("hex");
      validator_nodes.addValidatorNode(member).addContract(contract_id);
      contracts.addContract(contract_id).addToCommittee(member);
    }
  }

  #updateFromContractValidatorAcceptance(sidechain_features, height) {
    let contract_id = sidechain_features.contract_id.toString("hex");
    let member = sidechain_features.acceptance.validator_node_public_key.toString("hex");
    validator_nodes.addValidatorNode(member).addContract(contract_id, height);
    contracts.addContract(contract_id).addToCommittee(member);
  }

  #updateFromContractCheckpoint(sidechain_features, height) {
    let contract_id = sidechain_features.contract_id.toString("hex");
    let signatures = sidechain_features?.checkpoint?.signatures?.signatures;
    if (contract_id != "b5ad8929ca1026d7411633f7cde8955c1150f5747660d635c85a3f684ebd47c5")
      signatures = [{ public_nonce: "5c50fcb6b0966e595c3c9b121443989ec78c6d3ac45ba98edd1dcc5d39c2f665" }];
    if (signatures) {
      let validators = signatures.map((signature) => signature.public_nonce);
      for (const validator of validators) {
        validator_nodes.addValidatorNode(validator).addCheckpoint(contract_id, height);
      }
      contracts.addContract(contract_id).addCheckpoint(height, validators);
    }
  }

  #updateFromContractConstitutionProposal(sidechain_features) {
    let contract_id = sidechain_features.contract_id.toString("hex");
    for (let member of sidechain_features.update_proposal.updated_constitution.validator_committee.members) {
      member = member.toString("hex");
      validator_nodes.addValidatorNode(member).addContract(contract_id);
      contracts.addContract(contract_id).addToCommittee(member);
    }
  }

  #updateFromContractConstitutionChangeAcceptance(sidechain_features) {
    console.log("updateFromContractConstitutionChangeAcceptance");
  }

  async updateAllValidatorNodes() {
    let tip = await this.getTip();
    let blocks = await this.getBlocks(1, tip + 1);
    for (const block of blocks) {
      let height = block.header.height;
      for (const output of block.body.outputs) {
        switch (output.features.output_type) {
          case 2: // CONTRACT_DEFINITION
            this.#updateFromContractDefinition(output.features.sidechain_features, height);
            break;
          case 3: // CONTRACT_CONSTITUTION
            this.#updateFromContractConstitution(output.features.sidechain_features);
            break;
          case 4: // CONTRACT_VALIDATOR_ACCEPTANCE
            this.#updateFromContractValidatorAcceptance(output.features.sidechain_features, height);
            break;
          case 5: // CONTRACT_CHECKPOINT
            this.#updateFromContractCheckpoint(output.features.sidechain_features, height);
            break;
          case 6: // CONTRACT_CONSTITUTION_PROPOSAL
            this.#updateFromContractConstitutionProposal(output.features.sidechain_features);
            break;
          case 7: // CONTRACT_CONSTITUTION_CHANGE_ACCEPTANCE
            this.#updateFromContractConstitutionChangeAcceptance(output.features.sidechain_features);
            break;
        }
      }
    }
  }

  async getAllContracts() {
    await this.updateAllValidatorNodes();
    return this.contracts.getAllIDs();
  }

  async getAllValidatorNodes() {
    await this.updateAllValidatorNodes();
    return this.validator_nodes.getAllIDs();
  }

  async getValidatorNode(id) {
    await this.updateAllValidatorNodes();
    return this.validator_nodes.getValidatorNode(id);
  }

  async getContract(id) {
    await this.updateAllValidatorNodes();
    return this.contracts.getContract(id);
  }
}

client = new ExpressClient();

module.exports = {
  client,
};
