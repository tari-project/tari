const { spawn } = require('child_process');
const { expect } = require('chai');
const fs = require('fs');
const path = require('path');
const BaseNodeClient = require("./baseNodeClient");
const { getFreePort } = require("./util");
const dateFormat = require('dateformat');
const { createEnv } = require("./config");

let outputProcess;
class BaseNodeProcess {
    constructor(name, options, logFilePath, nodeFile) {
        this.name = name;
        this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
        this.nodeFile = nodeFile;
        this.options = options;
    }

    async init() {
        this.port = await getFreePort(19000, 25000);
        this.grpcPort = await getFreePort(19000, 25000);
        this.name = `Basenode${this.port}-${this.name}`;
        this.nodeFile = this.nodeFile || "nodeid.json";
        this.baseDir = `./temp/base_nodes/${dateFormat(new Date(), "yyyymmddHHMM")}/${this.name}`;
        const args = ["--base-path", ".", "--init", "--create-id"];
        if (this.logFilePath) {
            args.push("--log-config", this.logFilePath);
        }

        await this.run(await this.compile(), args);
        // console.log("Port:", this.port);
        // console.log("GRPC:", this.grpcPort);
        // console.log(`Starting node ${this.name}...`);
    }

    async compile() {
        if (!outputProcess) {
            await this.run("cargo", ["build", "--release", "--bin", "tari_base_node", "-Z", "unstable-options", "--out-dir", __dirname + "/../temp/out"]);
            outputProcess = __dirname + "/../temp/out/tari_base_node";
        }
        return outputProcess;
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
        // console.log("Base Node GRPC Address:",address);
        return address;
    }

    run(cmd, args, saveFile) {
        return new Promise((resolve, reject) => {
            if (!fs.existsSync(this.baseDir)) {
                fs.mkdirSync(this.baseDir, { recursive: true });
                fs.mkdirSync(this.baseDir + "/log", { recursive: true });
            }

            let envs = createEnv(this.name, false, this.nodeFile, "127.0.0.1", "8082", "8081", "127.0.0.1",
                this.grpcPort, this.port, "127.0.0.1:8080", this.options, this.peerSeeds);

            var ps = spawn(cmd, args, {
                cwd: this.baseDir,
                // shell: true,
                env: { ...process.env, ...envs }
            });

            ps.stdout.on('data', (data) => {
                //console.log(`stdout: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
                if (data.toString().match(/Copyright 2019-2020. The Tari Development Community/)) {
                    resolve(ps);
                }
            });

            ps.stderr.on('data', (data) => {
                console.error(`stderr: ${data}`);
                fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
            });

            ps.on('close', (code) => {
                let ps = this.ps;
                this.ps = null;
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

    async start() {
        const args = ["--base-path", "."];
        if (this.logFilePath) {
            args.push("--log-config", this.logFilePath);
        }
        return await this.run(await this.compile(), args);
    }

    stop() {
        return new Promise((resolve) => {
            if (!this.ps) {
                return resolve();
            }
            this.ps.on('close', (code) => {
                if (code) {
                    console.log(`child process exited with code ${code}`);
                }
                resolve();
            });
            this.ps.kill("SIGINT");
        });
    }

    createGrpcClient() {
        return new BaseNodeClient(this.grpcPort);
    }
}

module.exports = BaseNodeProcess;
