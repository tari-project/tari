// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const axios = require("axios");

class StratumTranscoderClient {
  constructor(address) {
    this.address = address;
    this.lastResponse = null;
  }

  async getLastResponse() {
    return this.lastResponse;
  }

  async getInfo() {
    const res = await axios.get(`${this.address}/get_info`);
    this.lastResponse = res.data;
  }

  async transferFunds(destinations) {
    try {
      let recipients = [];
      for (let i = 0; i < destinations.length; i++) {
        let recipient = {
          address: destinations[i].address,
          amount: destinations[i].amount,
          fee_per_gram: 25,
          message: "Test transfer",
        };
        recipients.push(recipient);
      }
      let req = {
        jsonrpc: "2.0",
        id: "0",
        method: "transfer",
        params: {
          recipients: recipients,
        },
      };
      const res = await axios.post(`${this.address}/json_rpc`, req);
      this.lastResponse = res.data;
    } catch (e) {
      console.error("getBlockTemplate error: ", e);
      throw e;
    }
  }

  async getBlockTemplate() {
    try {
      const res = await axios.post(`${this.address}/json_rpc`, {
        jsonrpc: "2.0",
        id: "0",
        method: "getblocktemplate",
        params: {},
      });
      this.lastResponse = res.data;
    } catch (e) {
      console.error("getBlockTemplate error: ", e);
      throw e;
    }
  }

  async submitBlock(block) {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "submit_block",
      params: [block],
    });
    this.lastResponse = res.data;
  }

  async getLastBlockHeader() {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "get_last_block_header",
      params: {},
    });
    this.lastResponse = res.data;
  }

  async getBlockHeaderByHeight(height) {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "get_block_header_by_height",
      params: {
        height: height,
      },
    });
    this.lastResponse = res.data;
  }

  async getBlockHeaderByHash(hash) {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "get_block_header_by_hash",
      params: {
        hash: hash,
      },
    });
    this.lastResponse = res.data;
  }

  async getBalance() {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "get_balance",
      params: {},
    });
    this.lastResponse = res.data;
  }
}

module.exports = StratumTranscoderClient;
