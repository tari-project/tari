const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
var fs = require('fs');
const BaseNodeClient = require("./baseNodeClient");
const {sleep, getRandomInt} = require("./util");

class BaseNodeProcess {
    constructor(name, nodeFile) {
        this.port = getRandomInt(19000, 20000);
        this.grpcPort = getRandomInt(50000, 51000);
        this.name = name || "Basenode" + this.port;
        this.nodeFile = nodeFile || "newnode_id.json";

        this.baseDir = "./temp/base_nodes/" + this.name;
        console.log("POrt:", this.port);
        console.log("GRPC:", this.grpcPort);
    }


    init() {
        return this.runSync("cargo",

            ["run", "--release", "--bin", "tari_base_node", "--", "--base-path", ".", "--create-id", "--init"]);
    }


    ensureNodeInfo() {
        this.nodeInfo = JSON.parse(fs.readFileSync(this.baseDir + "/" + this.nodeFile, 'utf8'));
    }

    peerAddress() {
        this.ensureNodeInfo();
        const addr = this.nodeInfo.public_key + "::" + this.nodeInfo.public_address;
        console.log("Peer:", addr);
        return addr;
    }

    setPeerSeeds(addresses) {
        this.peerSeeds = addresses.join(",");
    }


    createEnvs() {
        let envs = {

            TARI_BASE_NODE__NETWORK: "localnet",
            TARI_BASE_NODE__LOCALNET__DATA_DIR: "localnet",
            TARI_BASE_NODE__LOCALNET__DB_TYPE: "lmdb",
            TARI_BASE_NODE__LOCALNET__ORPHAN_STORAGE_CAPACITY: "10",
            TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON: "0",
            TARI_BASE_NODE__LOCALNET__PRUNED_MODE_CLEANUP_INTERVAL: "10000",
            TARI_BASE_NODE__LOCALNET__CORE_THREADS: "10",
            TARI_BASE_NODE__LOCALNET__MAX_THREADS: "512",
            TARI_BASE_NODE__LOCALNET__IDENTITY_FILE: this.nodeFile,
            TARI_BASE_NODE__LOCALNET__TOR_IDENTITY_FILE: "node_tor_id.json",
            TARI_BASE_NODE__LOCALNET__WALLET_IDENTITY_FILE: "walletid.json",
            TARI_BASE_NODE__LOCALNET__WALLET_TOR_IDENTITY_FILE: "wallet_tor_id.json",
            TARI_BASE_NODE__LOCALNET__TRANSPORT: "tcp",
            TARI_BASE_NODE__LOCALNET__TCP_LISTENER_ADDRESS: "/ip4/0.0.0.0/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__ALLOW_TEST_ADDRESSES: 'true',
            TARI_BASE_NODE__LOCALNET__PUBLIC_ADDRESS: "/ip4/10.0.0.102/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__GRPC_ENABLED: "true",
            TARI_BASE_NODE__LOCALNET__GRPC_ADDRESS: "127.0.0.1:" + this.grpcPort,
            TARI_BASE_NODE__LOCALNET__BLOCK_SYNC_STRATEGY: "ViaBestChainMetadata",
            TARI_BASE_NODE__LOCALNET__ENABLE_MINING: "false",
            TARI_BASE_NODE__LOCALNET__NUM_MINING_THREADS: "1",
            TARI_BASE_NODE__LOCALNET__ORPHAN_DB_CLEAN_OUT_THRESHOLD: "0",
            TARI_BASE_NODE__LOCALNET__GRPC_WALLET_ADDRESS: "127.0.0.1:5999",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_URL: "aasdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USE_AUTH: "false",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USERNAME: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_PASSWORD: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_HOST_ADDRESS: "127.0.0.1:50071"
        };

        if (this.peerSeeds) {
            envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = this.peerSeeds;
        }
        return envs;
    }


    runSync(cmd, args) {

        if (!fs.existsSync(this.baseDir)) {
            fs.mkdirSync(this.baseDir, {recursive: true});
        }
        var ps = spawnSync(cmd, args, {
            cwd: this.baseDir,
            shell: true,
            env: {...process.env, ...this.createEnvs()}
        });

        expect(ps.error).to.be.an('undefined');

        return ps;

    }

    run(cmd, args) {
        if (!fs.existsSync(this.baseDir)) {
            fs.mkdirSync(this.baseDir, {recursive: true});
        }
        var ps = spawn(cmd, args, {
            cwd: this.baseDir,
            shell: true,
            env: {...process.env, ...this.createEnvs()}
        });

        ps.stdout.on('data', (data) => {
            //console.log(`stdout: ${data}`);
        });

        ps.stderr.on('data', (data) => {
            console.error(`stderr: ${data}`);
        });

        ps.on('close', (code) => {
            console.log(`child process exited with code ${code}`);
        });

        expect(ps.error).to.be.an('undefined');
        return ps;

    }

    async startNew() {
        await this.init();
        return this.start();
    }

    async startAndConnect() {
        await this.startNew();
        return this.createGrpcClient();
    }

    async start() {
        var ps = this.run("cargo", ["run", "--release", "--bin tari_base_node", "--", "--base-path", "."]);
        await sleep(6000);
        return ps;
    }

    createGrpcClient() {
        return new BaseNodeClient(this.grpcPort);
    }
}

module.exports = BaseNodeProcess;
