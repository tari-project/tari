const { setWorldConstructor, After,BeforeAll } = require("cucumber");

const BaseNodeProcess = require('../../helpers/baseNodeProcess');
const MergeMiningProxyProcess = require('../../helpers/mergeMiningProxyProcess');
const WalletProcess = require('../../helpers/walletProcess');

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
        let result  = await this.clients[nodeName].submitBlock(this.blocks[blockName].block).catch(err =>  {
            console.log("submit block erro", err);
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


BeforeAll({timeout: 1200000}, function(callback) {
    // Ensure the project can compile
    let proc  =new BaseNodeProcess(`compile-tester`);
    console.log("Precompiling node. This can take a while whenever the code changes...");
    proc.startNew().then(function() {
        proc.stop();
        let proc2  =new MergeMiningProxyProcess(`compile-tester2`, "127.0.0.1:9999", "127.0.0.1:9998");
        console.log("Precompiling mmproxy. This can take a while whenever the code changes...");
        proc2.startNew().then(function() {
            proc2.stop();
            let proc3  =new WalletProcess(`compile-tester3`);
            console.log("Precompiling wallet. This can take a while whenever the code changes...");
            proc3.startNew().then(function() {
                proc3.stop();
                console.log("Finished check...");
                callback();
            });
        });
    });


});

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
