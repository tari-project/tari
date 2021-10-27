const { spawn } = require("child_process");
const { expect } = require("chai");
const fs = require("fs");
const path = require("path");
const ValidatorNodeClient = require("./validatorNodeClient");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { createEnv } = require("./config");

let outputProcess;
class ValidatorNodeProcess {
  constructor(name, excludeTestEnvars, options, logFilePath, nodeFile) {
    this.name = name;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.nodeFile = nodeFile;
    this.options = Object.assign(
      {
        baseDir: "./temp/base_nodes",
      },
      options || {}
    );
    this.excludeTestEnvars = excludeTestEnvars;
  }

  async init() {
    this.port = await getFreePort();
    this.grpcPort = 18080; // Currently it's constant
    this.name = `ValidatorNode${this.port}-${this.name}`;
    this.nodeFile = this.nodeFile || "nodeid.json";

    let instance = 0;
    do {
      this.baseDir = `${this.options.baseDir}/${dateFormat(
        new Date(),
        "yyyymmddHHMM"
      )}/${instance}/${this.name}`;
      // Some tests failed during testing because the next base node process started in the previous process
      // directory therefore using the previous blockchain database
      if (fs.existsSync(this.baseDir)) {
        instance++;
      } else {
        fs.mkdirSync(this.baseDir, { recursive: true });
        break;
      }
    } while (fs.existsSync(this.baseDir));
    const args = ["--base-path", ".", "--init", "--create-id"];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }

    await this.run(await this.compile(), args);
  }

  async compile() {
    if (!outputProcess) {
      await this.run("cargo", [
        "build",
        "--release",
        "--bin",
        "tari_validator_node",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out",
      ]);
      outputProcess = process.cwd() + "/temp/out/tari_validator_node";
    }
    return outputProcess;
  }

  hasAssetData() {
    return fs.existsSync(this.baseDir + "/localnet/asset_data");
  }

  ensureNodeInfo() {
    for (;;) {
      if (fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
        break;
      }
    }

    this.nodeInfo = JSON.parse(
      fs.readFileSync(this.baseDir + "/" + this.nodeFile, "utf8")
    );
  }

  peerAddress() {
    this.ensureNodeInfo();
    const addr = this.nodeInfo.public_key + "::" + this.nodeInfo.public_address;
    return addr;
  }

  getPubKey() {
    this.ensureNodeInfo();
    return this.nodeInfo.public_key;
  }

  setPeerSeeds(addresses) {
    this.peerSeeds = addresses.join(",");
  }

  setForceSyncPeers(addresses) {
    this.forceSyncPeers = addresses.join(",");
  }

  setCommittee(committee) {
    this.committee = committee.join(",");
  }

  getGrpcAddress() {
    const address = "127.0.0.1:" + this.grpcPort;
    // console.log("Base Node GRPC Address:",address);
    return address;
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir + "/log")) {
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      let envs = [];
      if (!this.excludeTestEnvars) {
        envs = createEnv(
          this.name,
          false,
          this.nodeFile,
          "127.0.0.1",
          "8082",
          "8081",
          "127.0.0.1",
          this.grpcPort,
          this.port,
          "127.0.0.1:8080",
          "127.0.0.1:8085",
          this.options,
          this.peerSeeds,
          "DirectAndStoreAndForward",
          this.forceSyncPeers,
          this.committee
        );
      }
      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (
          data
            .toString()
            .toUpperCase()
            .match(/STATE CHANGED FROM STARTING TO PREPARE/)
        ) {
          resolve(ps);
        }
      });

      ps.stderr.on("data", (data) => {
        // console.error(`stderr: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
      });

      ps.on("close", (code) => {
        const ps = this.ps;
        this.ps = null;
        if (code) {
          if (code == 101) {
            resolve(ps); // Validator node will fail, because of the missing committee section, but that's okay.
          } else {
            console.log(`child process exited with code ${code}`);
            reject(`child process exited with code ${code}`);
          }
        } else {
          resolve(ps);
        }
      });

      expect(ps.error).to.be.an("undefined");
      this.ps = ps;
    });
  }

  async startNew() {
    await this.init();
    const start = await this.start();
    return start;
  }

  async startAndConnect() {
    await this.startNew();
    return await this.createGrpcClient();
  }

  async start(opts = []) {
    const args = ["--base-path", "."];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    args.push(...opts);
    return await this.run(await this.compile(), args);
  }

  stop() {
    return new Promise((resolve) => {
      if (!this.ps) {
        return resolve();
      }
      this.ps.on("close", (code) => {
        if (code) {
          console.log(`child process exited with code ${code}`);
        }
        resolve();
      });
      this.ps.kill("SIGINT");
    });
  }

  async createGrpcClient() {
    return await ValidatorNodeClient.create(this.grpcPort);
  }
}

module.exports = ValidatorNodeProcess;
