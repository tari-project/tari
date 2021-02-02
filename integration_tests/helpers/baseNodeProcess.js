const {spawnSync, spawn, execSync} = require('child_process');
const {expect} = require('chai');
const fs = require('fs');
const BaseNodeClient = require("./baseNodeClient");
const {getFreePort} = require("./util");
const dateFormat = require('dateformat');
const {createEnv} = require("./config");

class BaseNodeProcess {
    constructor(name, options, nodeFile) {
        this.name = name;
        this.nodeFile = nodeFile;
        this.options = options;
        // this.port = getFreePort(19000, 25000);
        // this.grpcPort = getFreePort(50000, 51000);
        // this.name = `Basenode${this.port}-${name}`;
        // this.nodeFile = nodeFile || "newnode_id.json";
        // this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddhhMM")}/${this.name}`;
        // console.log("POrt:", this.port);
        // console.log("GRPC:", this.grpcPort);
    }


    async init() {
        this.port = await getFreePort(19000, 25000);
        this.grpcPort = await getFreePort(19000, 25000);
        this.name = `Basenode${this.port}-${this.name}`;
        this.nodeFile = this.nodeFile || "nodeid.json";
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
        await this.run("cargo",["run", "--release", "--bin", "tari_base_node", "--", "--base-path", ".", "--init", "--create-id"]);
        // console.log("POrt:", this.port);
        // console.log("GRPC:", this.grpcPort);
        // console.log(`Starting node ${this.name}...`);

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

    getGrpcAddress() {
        let address = "127.0.0.1:" + this.grpcPort;
        console.log("Base Node GRPC Address:",address);
        return address;
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

    run(cmd, args, saveFile) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, {recursive: true});
                fs.mkdirSync(this.baseDir + "/log", {recursive: true});
            }

            let envs = createEnv(this.name,false, this.nodeFile,"127.0.0.1", "8082","8081","127.0.0.1",this.grpcPort,this.port,"127.0.0.1:8080",this.options,this.peerSeeds);

            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                shell: true,
                env: {...process.env, ...envs}
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
        return await this.start();
    }

    async startAndConnect() {
        await this.startNew();
        return this.createGrpcClient();
    }

    async start () {
        return await this.run("cargo",["run", "--release", "--bin", "tari_base_node", "--", "--base-path", "."]);
    }

    stop() {
        this.ps.kill("SIGINT");
    }

    createGrpcClient() {
        return new BaseNodeClient(this.grpcPort);
    }
}

module.exports = BaseNodeProcess;
