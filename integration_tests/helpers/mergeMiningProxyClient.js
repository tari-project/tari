const axios = require('axios');


class MergeMiningProxyClient {

    constructor(address) {
        this.address = address;
    }

    async getHeight() {
        let res = await axios.get(`${this.address}/get_height`);
        console.log("Merge Mining Height:",res.data.height);
        return res.data.height;
    }

    async getBlockTemplate() {
        let res = await axios.post(`${this.address}/json_rpc`, {
            "jsonrpc": "2.0",
            "id": "0",
            "method": "getblocktemplate",
            "params": {
                "wallet_address": "55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt",
                "reserve_size": 60
            }
        });
        console.log("Blocktemplate:",res.data.result.blocktemplate_blob);
        return res.data.result;
    }

    async submitBlock(block) {
        let res = await axios.post(`${this.address}/json_rpc`, {
            "jsonrpc": "2.0",
            "id": "0",
            "method": "submit_block",
            "params": [block]
        });
        return res.data;
    }

    async mineBlock() {
        // Mines a block in the same way that xmrig would
       let template = await this.getBlockTemplate();
       let height = await this.getHeight();
       let block = template.blocktemplate_blob;
        // Need to insert a nonce into the template as xmrig would for it to be a valid block.
       let result = await this.submitBlock(block);
    }

}

module.exports = MergeMiningProxyClient;
