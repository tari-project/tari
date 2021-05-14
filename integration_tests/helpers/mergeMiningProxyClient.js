const axios = require("axios");

class MergeMiningProxyClient {
  constructor(address) {
    this.address = address;
  }

  async getHeight() {
    const res = await axios.get(`${this.address}/get_height`);
    return res.data.height;
  }

  async getBlockTemplate() {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "getblocktemplate",
      params: {
        wallet_address:
          "55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt",
        reserve_size: 60,
      },
    });
    // console.log(res.data);
    // console.log("Blocktemplate:",res.data.result.blocktemplate_blob);
    return res.data.result;
  }

  async submitBlock(block) {
    const res = await axios.post(`${this.address}/json_rpc`, {
      jsonrpc: "2.0",
      id: "0",
      method: "submit_block",
      params: [block],
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
    // const height = await this.getHeight();
    const block = template.blocktemplate_blob;
    // Need to insert a nonce into the template as xmrig would for it to be a valid block.
    await this.submitBlock(block);
  }
}

module.exports = MergeMiningProxyClient;
