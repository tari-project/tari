// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { spawn } = require("child_process");
const { expect } = require("chai");
const fs = require("fs");
const path = require("path");
const BaseNodeClient = require("./baseNodeClient");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { createEnv } = require("./config");
const JSON5 = require("json5");

let outputProcess;
class BaseNodeProcess {
  constructor(name, excludeTestEnvars, options, logFilePath, nodeFile) {
    this.name = name;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.nodeFile = nodeFile;
    this.peerSeeds = [];
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
    this.grpcPort = await getFreePort();
    this.name = `Basenode${this.port}-${this.name}`;
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
        console.log("Making base dir ", this.baseDir);
        fs.mkdirSync(`${this.baseDir}/log`, { recursive: true });
        break;
      }
    } while (fs.existsSync(this.baseDir));
    const args = [
      "--base-path",
      ".",
      "--init",
      "--create-id",
      "--network",
      "localnet",
    ];
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }

    await this.run(await this.compile(), args);
    console.log("Port:", this.port);
    console.log("GRPC:", this.grpcPort);
    console.log(`Starting node ${this.name}...`);
  }

  async compile() {
    if (!outputProcess) {
      await this.runCommand("cargo", [
        "build",
        "--release",
        "--bin",
        "tari_base_node",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out",
      ]);
      outputProcess = process.cwd() + "/temp/out/tari_base_node";
    }
    return outputProcess;
  }

  ensureNodeInfo() {
    for (let i = 0; i < 100; i++) {
      if (fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
        break;
      }
    }
    if (!fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
      throw new Error(
        `Node id file node found ${this.baseDir}/${this.nodeFile}`
      );
    }

    this.nodeInfo = JSON5.parse(
      fs.readFileSync(this.baseDir + "/" + this.nodeFile, "utf8")
    );
  }

  peerAddress() {
    this.ensureNodeInfo();
    const addr = this.nodeInfo.public_key + "::" + this.nodeInfo.public_address;
    // console.log("Peer:", addr);
    return addr;
  }

  setPeerSeeds(addresses) {
    this.peerSeeds = addresses;
  }

  setForceSyncPeers(addresses) {
    this.forceSyncPeers = addresses;
  }

  getOverrides() {
    return createEnv({
      network: "localnet",
      baseNodeGrpcAddress: this.getGrpcAddress(),
      isWallet: false,
      baseNodePort: this.port,
      peerSeeds: this.peerSeeds,
      forceSyncPeers: this.forceSyncPeers,
      nodeFile: this.nodeFile,
      options: this.options,
    });
  }

  getGrpcAddress() {
    const address = "/ip4/127.0.0.1/tcp/" + this.grpcPort;
    // console.log("Base Node GRPC Address:",address);
    return address;
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir + "/log")) {
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      let overrides = this.excludeTestEnvars ? {} : this.getOverrides();
      console.error(overrides);

      // clear the .env file
      fs.writeFileSync(`${this.baseDir}/.overrides`, "");
      Object.keys(overrides).forEach((key) => {
        fs.appendFileSync(
          `${this.baseDir}/.overrides`,
          ` -p ${key}=${overrides[key]}`
        );
      });

      // Create convenience script - this is NOT used to start the base node in cucumber
      fs.writeFileSync(
        `${this.baseDir}/start_node.sh`,
        "bash -c \"RUST_BACKTRACE=1 cargo run --release --bin tari_base_node -- -n --watch status -b . --network localnet $(grep -v '^#' .overrides)\"",
        { mode: 0o777 }
      );

      // console.log("Running command ", cmd, args.join(" "));
      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, RUST_BACKTRACE: "1" },
      });

      ps.stdout.on("data", (data) => {
        //console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (
          // Make this resilient by comparing uppercase and making provisioning that the first print message in the
          // base node console is not always 'State: Starting up'
          data
            .toString()
            .toUpperCase()
            .match(/STATE: STARTING/) ||
          data
            .toString()
            .toUpperCase()
            .match(/STATE: LISTENING/) ||
          data
            .toString()
            .toUpperCase()
            .match(/STATE: SYNCING/)
        ) {
          resolve(ps);
        }
      });

      ps.stderr.on("data", (data) => {
        console.error(`stderr: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
      });

      ps.on("close", (code) => {
        const ps = this.ps;
        this.ps = null;
        if (code) {
          console.log(`child process exited with code ${code}`);
          reject(`child process exited with code ${code}`);
        } else {
          resolve(ps);
        }
      });

      expect(ps.error).to.be.an("undefined");
      this.ps = ps;
    });
  }

  runCommand(cmd, args, opts = { env: {} }) {
    return new Promise((resolve, reject) => {
      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...opts.env },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        resolve(ps);
      });

      ps.stderr.on("data", (data) => {
        console.error(`stderr: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stderr.log`, data.toString());
      });

      ps.on("close", (code) => {
        const ps = this.ps;
        this.ps = null;
        if (code) {
          console.log(`child process exited with code ${code}`);
          reject(`child process exited with code ${code}`);
        } else {
          resolve(ps);
        }
      });
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
    const args = [
      "--non-interactive-mode",
      "--watch",
      "status",
      "--base-path",
      ".",
      "--network",
      "localnet",
    ];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    args.concat(opts);
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    let cmd = await this.compile();
    return await this.run(cmd, args);
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
    return await BaseNodeClient.create(this.grpcPort);
  }
}

module.exports = BaseNodeProcess;
