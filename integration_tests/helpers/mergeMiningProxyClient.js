// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const axios = require("axios");

class MergeMiningProxyClient {
  constructor(address, nodeClient) {
    this.address = address;
    this.baseNodeClient = nodeClient;
  }

  async getHeight() {
    const res = await axios.get(`${this.address}/get_height`);
    return res.data.height;
  }

  async getBlockTemplate() {
    try {
      const res = await axios.post(`${this.address}/json_rpc`, {
        jsonrpc: "2.0",
        id: "0",
        method: "getblocktemplate",
        params: {
          wallet_address:
            "5AUoj81i63cBUbiKY5jybsZXRDYb9CppmSjiZXC8ZYT6HZH6ebsQvBecYfRKDYoyzKF2uML9FKkTAc7nJvHKdoDYQEeteRW",
          reserve_size: 60,
        },
      });
      // console.log(res.data);
      // console.log("Blocktemplate:", res.data.result.blocktemplate_blob);
      return res.data.result;
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
      timeout: 60,
    });
    return res.data;
  }

  async getLastBlockHeader() {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "get_last_block_header",
    });
    return res.data;
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
    return res.data;
  }

  async mineBlock() {
    // Mines a block in the same way that xmrig would
    const template = await this.getBlockTemplate();
    // XMRig always calls this, so duplicated here
    await this.getHeight();
    const block = template.blocktemplate_blob;
    // Need to insert a nonce into the template as xmrig would for it to be a valid block.
    await this.submitBlock(block);
  }

  async mineBlocksUntilHeightIncreasedBy(numBlocks) {
    let tipHeight = parseInt(await this.baseNodeClient.getTipHeight());
    const height = tipHeight + parseInt(numBlocks);
    let i = 0;
    do {
      if (i % 25 === 0) {
        console.log(
          "[mmProxy client] Tip at",
          tipHeight,
          "...(stopping at " + height + ")"
        );
      }
      i += 1;
      // Mines a block in the same way that xmrig would
      const template = await this.getBlockTemplate();
      const block = template.blocktemplate_blob;
      // Need to insert a nonce into the template as xmrig would for it to be a valid block.
      tipHeight = parseInt(await this.baseNodeClient.getTipHeight());
      if (tipHeight >= height) {
        break;
      }
      await this.submitBlock(block);
    } while (tipHeight + 1 < height);
    tipHeight = await this.baseNodeClient.getTipHeight();
    console.log("[mmProxy client] Tip is at target height", tipHeight);
    return tipHeight;
  }
}

module.exports = MergeMiningProxyClient;
