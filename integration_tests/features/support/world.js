const { setWorldConstructor, After } = require("cucumber");

// const BaseNodeClient = require('../helpers/baseNodeClient');
// const TransactionBuilder = require('../helpers/transactionBuilder');
const BaseNodeProcess = require('../../helpers/baseNodeProcess');

class CustomWorld {
    constructor() {
        //this.variable = 0;
        this.seeds = {};
        this.nodes = {};
        this.clients ={};
        this.headers = {};

    }

    async createSeedNode(name) {
        let proc = new BaseNodeProcess();
        await proc.startNew();
        this.seeds[name] = proc;
        this.clients[name] = proc.createGrpcClient();
    }

    addNode(name, process) {
        this.nodes[name] = process;
        this.clients[name] = process.createGrpcClient();
    }

    async mineBlock(name, beforeSubmit, onError) {
        await this.clients[name].mineBlockWithoutWallet(beforeSubmit, onError);
    }

    getClient(name) {
        return this.clients[name];
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
    for (const property in this.seeds) {
        console.log("Stopping seed", property);
        this.stopNode(property);
    }
    for (const property in this.nodes) {
        console.log("Stopping node", property);
        this.stopNode(property);
    }
});
