// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { spawn } = require("child_process");
const { expect } = require("chai");
const fs = require("fs");
const path = require("path");
const ValidatorNodeClient = require("./validatorNodeClient");
const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const { createEnv } = require("./config");
const JSON5 = require("json5");

let outputProcess;
class ValidatorNodeProcess {
  constructor(
    name,
    excludeTestEnvars,
    options,
    logFilePath,
    nodeFile,
    baseNodeAddress,
    walletAddress
  ) {
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
    this.baseNodeAddress = baseNodeAddress;
    this.walletAddress = walletAddress;
  }

  async init() {
    this.port = await getFreePort();
    this.grpcPort = await getFreePort();
    this.name = `ValidatorNode${this.port}-${this.name}`;
    this.nodeFile = this.nodeFile || "validator_node_id.json";

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
    const args = ["--base-path", "."];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }

    await this.compile();
    // await this.run(cmd, args);
  }

  async compile() {
    if (!outputProcess) {
      await new Promise((resolve, reject) => {
        const ps = spawn(
          "cargo",
          [
            "build",
            "--release",
            "--bin",
            "tari_validator_node",
            "-Z",
            "unstable-options",
            "--out-dir",
            process.cwd() + "/temp/out",
          ],
          {
            cwd: this.baseDir,
            // shell: true,
            env: { ...process.env },
          }
        );

        ps.on("close", (code) => {
          const ps = this.ps;
          this.ps = null;
          if (code) {
            reject(`child process exited with code ${code}`);
          } else {
            resolve(ps);
          }
        });

        expect(ps.error).to.be.an("undefined");
        this.ps = ps;
      });

      outputProcess = process.cwd() + "/temp/out/tari_validator_node";
    }
    return outputProcess;
  }

  hasAssetData() {
    return fs.existsSync(this.baseDir + "/localnet/asset_data");
  }

  ensureNodeInfo() {
    for (let i = 0; i < 100; i++) {
      if (fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
        break;
      }
    }
    if (!fs.existsSync(this.baseDir + "/" + this.nodeFile)) {
      throw new Error(
        `Node id file not found ${this.baseDir}/${this.nodeFile}`
      );
    }

    this.nodeInfo = JSON5.parse(
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
    const address = "/ip4/127.0.0.1/tcp/" + this.grpcPort;
    console.log("Validator Node GRPC Address:", address);
    return address;
  }

  run(cmd, args) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir + "/log")) {
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      // to avoid writing permission errors, we copy the reference identity file to the temp folder
      let identity_file_name = "validator_node_id.json";
      let identity_source_path = path.resolve(
        `./fixtures/${identity_file_name}`
      );
      let identity_destination_path = path.resolve(
        `${this.baseDir}/${identity_file_name}`
      );
      fs.copyFile(identity_source_path, identity_destination_path, (err) => {
        if (err) {
          console.log(
            "Error Found while copying validator identity file to temp folder: ",
            err
          );
          throw err;
        }
        console.log("Validator identity file was copied to destination");
        fs.chmod(identity_destination_path, 0o600, (err) => {
          if (err) {
            console.log(
              "Error Found while changing the permissions of the validator indentity file: ",
              err
            );
            throw err;
          }
          console.log(
            "Validator identity file permissions successfully modified"
          );
        });
      });

      let envs = [];
      if (!this.excludeTestEnvars) {
        envs = this.getOverrides();
      }

      let customArgs = {
        "validator_node.p2p.transport.type": "tcp",
      };
      if (this.baseNodeAddress) {
        customArgs["validator_node.base_node_grpc_address"] =
          this.baseNodeAddress;
      }
      if (this.baseNodeAddress) {
        customArgs["validator_node.wallet_grpc_address"] = this.walletAddress;
      }
      if (this.baseNodeAddress) {
        customArgs["validator_node.grpc_address"] = this.getGrpcAddress();
      }
      Object.keys(this.options).forEach((k) => {
        if (k.startsWith("validator_node.")) {
          customArgs[k] = this.options[k];
        }
      });

      Object.keys(customArgs).forEach((k) => {
        args.push("-p");
        args.push(`${k}=${customArgs[k]}`);
      });

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env, ...envs },
      });

      ps.stdout.on("data", (data) => {
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (
          data
            .toString()
            .toUpperCase()
            .match(/STARTING GRPC/)
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

  getOverrides() {
    return createEnv({
      network: "localnet",
      validatorNodeGrpcAddress: this.getGrpcAddress(),
      baseNodeGrpcAddress: this.baseNodeAddress,
      walletGrpcAddress: this.walletAddress,
      nodeFile: this.nodeFile,
      options: this.options,
      peerSeeds: this.peerSeeds,
      forceSyncPeers: this.forceSyncPeers,
    });
  }
}

module.exports = ValidatorNodeProcess;
