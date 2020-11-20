const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
var fs = require('fs');
const BaseNodeClient = require("./baseNodeClient");
const {getFreePort} = require("./util");
const dateFormat = require('dateformat');

class BaseNodeProcess {
    constructor(name, nodeFile) {
        this.name = name;
        this.nodeFile= nodeFile;
        // this.port = getFreePort(19000, 20000);
        // this.grpcPort = getFreePort(50000, 51000);
        // this.name = `Basenode${this.port}-${name}`;
        // this.nodeFile = nodeFile || "newnode_id.json";
        // this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddhhMM")}/${this.name}`;
        // console.log("POrt:", this.port);
        // console.log("GRPC:", this.grpcPort);
    }


    async init() {
                this.port = await getFreePort(19000, 20000);
                this.grpcPort = await getFreePort(50000, 51000);
                this.name = `Basenode${this.port}-${this.name}`;
                this.nodeFile = this.nodeFile || "newnode_id.json";
                this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
                // console.log("POrt:", this.port);
                // console.log("GRPC:", this.grpcPort);
        console.log(`Starting node ${this.name}...`);
        await this.run("cargo",

            ["run", "--bin", "tari_base_node", "--", "--base-path", ".", "--create-id", "--init"]);
    }


    ensureNodeInfo() {
        while (true) {
            if (fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
                break;
            }
        }

        this.nodeInfo = JSON.parse(fs.readFileSync(this.baseDir + "/" + this.nodeFile, 'utf8'));

    }

    peerAddress() {
        this.ensureNodeInfo();
        const addr = this.nodeInfo.public_key + "::" + this.nodeInfo.public_address;
        // console.log("Peer:", addr);
        return addr;
    }

    setPeerSeeds(addresses) {
        this.peerSeeds = addresses.join(",");
    }

    createEnvs() {
        let envs = {
            RUST_BACKTRACE: 1,
            TARI_BASE_NODE__NETWORK: "localnet",
            TARI_BASE_NODE__LOCALNET__DATA_DIR: "localnet",
            TARI_BASE_NODE__LOCALNET__DB_TYPE: "lmdb",
            TARI_BASE_NODE__LOCALNET__ORPHAN_STORAGE_CAPACITY: "10",
            TARI_BASE_NODE__LOCALNET__PRUNING_HORIZON: "0",
            TARI_BASE_NODE__LOCALNET__PRUNED_MODE_CLEANUP_INTERVAL: "10000",
            TARI_BASE_NODE__LOCALNET__CORE_THREADS: "10",
            TARI_BASE_NODE__LOCALNET__MAX_THREADS: "512",
            TARI_BASE_NODE__LOCALNET__IDENTITY_FILE: this.nodeFile,
            TARI_BASE_NODE__LOCALNET__BASE_NODE_IDENTITY_FILE: this.nodeFile,
            TARI_BASE_NODE__LOCALNET__BASE_NODE_TOR_IDENTITY_FILE: "node_tor_id.json",
            TARI_BASE_NODE__LOCALNET__WALLET_IDENTITY_FILE: "walletid.json",
            TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_IDENTITY_FILE: "cwalletid.json",
            TARI_BASE_NODE__LOCALNET__WALLET_TOR_IDENTITY_FILE: "wallet_tor_id.json",
            TARI_BASE_NODE__LOCALNET__CONSOLE_WALLET_TOR_IDENTITY_FILE: "wallet_tor_id.json",
            TARI_BASE_NODE__LOCALNET__TRANSPORT: "tcp",
            TARI_BASE_NODE__LOCALNET__TCP_LISTENER_ADDRESS: "/ip4/0.0.0.0/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__ALLOW_TEST_ADDRESSES: true,
            TARI_BASE_NODE__LOCALNET__PUBLIC_ADDRESS: "/ip4/10.0.0.104/tcp/" + this.port,
            TARI_BASE_NODE__LOCALNET__GRPC_ENABLED: "true",
            TARI_BASE_NODE__LOCALNET__ENABLE_WALLET: false,
            TARI_BASE_NODE__LOCALNET__GRPC_BASE_NODE_ADDRESS: "127.0.0.1:" + this.grpcPort,
            TARI_BASE_NODE__LOCALNET__GRPC_CONSOLE_WALLET_ADDRESS: "127.0.0.1:" + this.grpcPort,
            TARI_BASE_NODE__LOCALNET__DNS_SEEDS_NAME_SERVER: "1.1.1.1:53",
            TARI_BASE_NODE__LOCALNET__DNS_SEEDS_USE_DNSSEC: "true",
            TARI_BASE_NODE__LOCALNET__BLOCK_SYNC_STRATEGY: "ViaBestChainMetadata",
            TARI_BASE_NODE__LOCALNET__ENABLE_MINING: "false",
            TARI_BASE_NODE__LOCALNET__NUM_MINING_THREADS: "1",
            TARI_BASE_NODE__LOCALNET__ORPHAN_DB_CLEAN_OUT_THRESHOLD: "0",
            TARI_BASE_NODE__LOCALNET__GRPC_WALLET_ADDRESS: "127.0.0.1:5999",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_URL: "aasdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USE_AUTH: "false",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_USERNAME: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__MONEROD_PASSWORD: "asdf",
            TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_HOST_ADDRESS: "127.0.0.1:50071",
            TARI_BASE_NODE__LOCALNET__DB_INIT_SIZE_MB: 100,
            TARI_BASE_NODE__LOCALNET__DB_RESIZE_THRESHOLD_MB: 10,
            TARI_BASE_NODE__LOCALNET__DB_GROW_SIZE_MB: 20
            // Speeder peer selection
            // TARI_BASE_NODE__LOCALNET__CONNECTIVITY_UPDATE_INTERVAL: 10
        };
        if (this.peerSeeds) {
            envs.TARI_BASE_NODE__LOCALNET__PEER_SEEDS = this.peerSeeds;
        }
        return envs;
    }

    //
    // runSync(cmd, args) {
    //
    //     if (!fs.existsSync(this.baseDir)) {
    //         fs.mkdirSync(this.baseDir, {recursive: true});
    //     }
    //     var ps = spawnSync(cmd, args, {
    //         cwd: this.baseDir,
    //         shell: true,
    //         env: {...process.env, ...this.createEnvs()}
    //     });
    //
    //     expect(ps.error).to.be.an('undefined');
    //
    //     this.ps = ps;
    //     return ps;
    //
    // }

    run(cmd, args) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }
            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                shell: true,
                env: {...process.env, ...this.createEnvs()}
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                if (data.toString().match(/Copyright 2019-2020. The Tari Development Community/)) {
                    resolve(ps);
                }
            });

            ps.stderr.on('data', (data) => {
               // console.error(`stderr: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
            });

            ps.on('close', (code) => {
                if (code) {
                    console.log(`child process exited with code ${code}`);
                    reject(`child process exited with code ${code}`);
                } else {
                    resolve(ps);
                }
            });

            expect(ps.error).to.be.an('undefined');
            this.ps = ps;
        });
    }

    async startNew() {
        await this.init();
        return this.start();
    }

    async startAndConnect() {
        await this.startNew();
        return this.createGrpcClient();
    }

    start() {
        return this.run("cargo", ["run","--bin tari_base_node", "--", "--base-path", "."]);
    }

    stop() {
        this.ps.kill("SIGINT");
    }

    createGrpcClient() {
        return new BaseNodeClient(this.grpcPort);
    }
}

module.exports = BaseNodeProcess;
