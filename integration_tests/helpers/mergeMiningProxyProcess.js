// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const MergeMiningProxyClient = require("./mergeMiningProxyClient");
const { createEnv } = require("./config");
require("https");
require("http");

let outputProcess;

class MergeMiningProxyProcess {
  constructor(
    name,
    baseNodeAddress,
    baseNodeClient,
    walletAddress,
    logFilePath,
    submitOrigin = true
  ) {
    this.name = name;
    this.baseNodeAddress = baseNodeAddress;
    this.baseNodeClient = baseNodeClient;
    this.walletAddress = walletAddress;
    this.submitOrigin = submitOrigin;
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
  }

  async init() {
    this.port = await getFreePort();
    this.name = `MMProxy${this.port}-${this.name}`;
    this.baseDir = `./temp/base_nodes/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
    // console.log("MergeMiningProxyProcess init - assign server GRPC:", this.grpcPort);
  }

  async run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      const proxyFullAddress = "/ip4/127.0.0.1/tcp/" + this.port;

      const envs = createEnv({
        walletGrpcAddress: this.walletAddress,
        baseNodeGrpcAddress: this.baseNodeAddress,
        proxyFullAddress,
      });
      const extraEnvs = {
        TARI_MERGE_MINING_PROXY__PROXY_SUBMIT_TO_ORIGIN: this.submitOrigin,
      };
      const completeEnvs = { ...envs, ...extraEnvs };
      console.log(completeEnvs);
      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...completeEnvs },
      });

      ps.stdout.on("data", (data) => {
        // console.log(`stdout: ${data}`);
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (data.toString().match(/Listening on/)) {
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

  async startNew() {
    await this.init();
    const args = ["--base-path", ".", "--init"];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }

    return await this.run(await this.compile(), args);
  }

  async compile() {
    if (!outputProcess) {
      await this.run("cargo", [
        "build",
        "--release",
        "--bin",
        "tari_merge_mining_proxy",
        "-Z",
        "unstable-options",
        "--out-dir",
        __dirname + "/../temp/out",
      ]);
      outputProcess = __dirname + "/../temp/out/tari_merge_mining_proxy";
    }
    return outputProcess;
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

  createClient() {
    const address = "http://127.0.0.1:" + this.port;
    // console.log("MergeMiningProxyProcess createClient - client address:", address);
    return new MergeMiningProxyClient(address, this.baseNodeClient);
  }
}

module.exports = MergeMiningProxyProcess;
