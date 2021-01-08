const { setWorldConstructor, After,Before } = require("cucumber");

// const BaseNodeClient = require('../helpers/baseNodeClient');
// const TransactionBuilder = require('../helpers/transactionBuilder');
const BaseNodeProcess = require('../../helpers/baseNodeProcess');

class CustomWorld {
    constructor() {
        //this.variable = 0;
        this.seeds = {};
        this.nodes = {};
        this.proxies = {};
        this.wallets = {};
        this.clients = {};
        this.headers = {};
        this.outputs = {};
        this.testrun = `run${Date.now()}`;
        this.lastResult = null;
        this.blocks =  {};
        this.transactions = {};
        this.peers = {};
    }

    async createSeedNode(name) {
        let proc =  new BaseNodeProcess(`seed-${name}`);
        await proc.startNew();
        this.seeds[name] = proc;
        this.clients[name] = proc.createGrpcClient();
    }


    seedAddresses() {
        let res = [];
        for (const property in this.seeds) {
            res.push(this.seeds[property].peerAddress());
        }
        return res;
    }

    addNode(name, process) {
        this.nodes[name] = process;
        this.clients[name] = process.createGrpcClient();
    }

    addProxy(name, process) {
        this.proxies[name] = process;
    }

    addWallet(name, process) {
        this.wallets[name] = process;
    }

    addOutput(name, output) {
        this.outputs[name] = output;
    }

    async mineBlock(name, beforeSubmit, onError) {
        await this.clients[name].mineBlockWithoutWallet(beforeSubmit, onError);
    }

    async mergeMineBlock(name) {
        let client = this.proxies[name].createClient();
        await client.mineBlock();
    }

    saveBlock(name, block) {
        this.blocks[name] = block;
    }

    async submitBlock(blockName, nodeName) {
        let result  = await this.clients[nodeName].submitBlock(this.blocks[blockName]).catch(err =>  {
            console.log("erro", err);
        });
        console.log(result);
    }

    getClient(name) {
        return this.clients[name];
    }

    getNode(name) {
        return this.nodes[name] || this.seeds[name];
    }

    getWallet(name) {
        return this.wallets[name];
    }

    async forEachClientAsync(f) {
        let promises = [];

        for (const property in this.seeds) {
            promises.push(f(this.getClient(property), property));
        }
        for (const property in this.nodes) {
            promises.push(f(this.getClient(property), property));
        }
        await Promise.all(promises);
    }

    stopNode(name) {
        const node = this.seeds[name] || this.nodes[name];
        node.stop();
    }

    async startNode(name) {
        const node = this.seeds[name] || this.nodes[name];
        await node.start();
    }
}

setWorldConstructor(CustomWorld);

After(function () {
    console.log('Stopping nodes');
    for (const property in this.seeds) {
        this.stopNode(property);
    }
    for (const property in this.nodes) {
        this.stopNode(property);
    }
    for (const property in this.proxies) {
        this.proxies[property].stop();
    }
    for (const property in this.wallets) {
        this.wallets[property].stop();
    }
});
