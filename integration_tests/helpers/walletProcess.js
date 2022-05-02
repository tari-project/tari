// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

const { getFreePort } = require("./util");
const dateFormat = require("dateformat");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");
const { expect } = require("chai");
const { createEnv } = require("./config");
const WalletClient = require("./walletClient");
const csvParser = require("csv-parser");
var tari_crypto = require("tari_crypto");

let outputProcess;

class WalletProcess {
  constructor(name, excludeTestEnvars, options, logFilePath, seedWords) {
    this.name = name.toString();
    this.options = Object.assign(
      {
        baseDir: "./temp/base_nodes",
      },
      options || {}
    );
    this.logFilePath = logFilePath ? path.resolve(logFilePath) : logFilePath;
    this.recoverWallet = !!seedWords;
    this.seedWords = seedWords;
    this.excludeTestEnvars = excludeTestEnvars;
  }

  async init() {
    this.port = await getFreePort();
    this.name = `Wallet${this.port}-${this.name}`;
    this.grpcPort = await getFreePort();
    this.baseDir = `${this.options.baseDir}/${dateFormat(
      new Date(),
      "yyyymmddHHMM"
    )}/${this.name}`;
    this.seedWordsFile = path.resolve(this.baseDir + "/config/seed_words.log");
    if (!fs.existsSync(this.baseDir)) {
      fs.mkdirSync(this.baseDir + "/log", { recursive: true });
    }
  }

  getGrpcAddress() {
    return "/ip4/127.0.0.1/tcp/" + this.grpcPort;
  }

  async connectClient() {
    let client = new WalletClient(this.name);
    let addr = this.getGrpcAddress();
    console.log(`Connecting to ${addr} (${this.name})`);
    await client.connect(addr);
    return client;
  }

  getSeedWords() {
    try {
      return fs.readFileSync(this.seedWordsFile, "utf8");
    } catch (err) {
      console.error("\n", this.name, ": Seed words file not found!\n", err);
    }
  }

  setPeerSeeds(addresses) {
    this.peerSeeds = addresses;
  }

  run(cmd, args, saveFile, input_buffer, output, waitForCommand) {
    return new Promise((resolve, reject) => {
      let overrides = {};
      const network =
        this.options && this.options.network
          ? this.options.network.toLowerCase()
          : "localnet";
      overrides[`base_node.network`] = network;
      if (!this.excludeTestEnvars) {
        overrides = createEnv({
          network: "localnet",
          isWallet: true,
          nodeFile: "cwalletid.json",
          options: this.options,
          peerSeeds: this.peerSeeds,
          walletPort: this.port,
          walletGrpcAddress: this.getGrpcAddress(),
        });
      } else if (this.options["grpc_console_wallet_address"]) {
        overrides[`wallet.grpc_address`] =
          this.options["grpc_console_wallet_address"];
        let regexMatch =
          this.options["grpc_console_wallet_address"].match(/tcp\/(\d+)/);
        this.grpcPort = parseInt(regexMatch[1]);
      }
      console.log(`--------------------- ${this.name} ----------------------`);
      console.log(overrides);
      Object.keys(overrides).forEach((k) => {
        args.push("-p");
        args.push(`${k}=${overrides[k]}`);
      });
      if (saveFile) {
        // clear the .env file
        fs.writeFileSync(`${this.baseDir}/.overrides`, "");
        Object.keys(overrides).forEach((key) => {
          fs.appendFileSync(
            `${this.baseDir}/.overrides`,
            `-p ${key}=${overrides[key]}`
          );
        });
      }

      const ps = spawn(cmd, args, {
        cwd: this.baseDir,
        // shell: true,
        env: { ...process.env },
      });

      if (input_buffer) {
        // If we want to simulate user input we can do so here.
        ps.stdin.write(input_buffer);
      }
      ps.stdout.on("data", (data) => {
        console.log(`\nstdout: ${data}`);
        if (output !== undefined && output.buffer !== undefined) {
          output.buffer += data;
        }
        fs.appendFileSync(`${this.baseDir}/log/stdout.log`, data.toString());
        if (
          (!waitForCommand &&
            data.toString().match(/Tari Console Wallet running/i)) ||
          (waitForCommand &&
            data
              .toString()
              .match(
                /(?=.*Tari Console Wallet running)(?=.*Command mode completed)/gim
              ))
        ) {
          console.log("Wallet started");
          this.recoverWallet = false;
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
        if (code === 112) {
          reject("Incorrect password");
        } else if (code) {
          console.log(`child process exited with code ${code}`);
          reject(`child process exited with code ${code}`);
        } else {
          resolve(ps);
        }
      });
      expect(ps.error).to.be.undefined;
      this.ps = ps;
    });
  }

  async startNew() {
    await this.init();
    return await this.start();
  }

  getOverrides() {
    return createEnv({
      network: "localnet",
      walletGrpcAddress: this.getGrpcAddress(),
      isWallet: true,
      walletPort: this.port,
      peerSeeds: this.peerSeeds,
    });
  }

  async compile() {
    if (!outputProcess) {
      let args = [
        "build",
        "--release",
        "--bin",
        "tari_console_wallet",
        "-Z",
        "unstable-options",
        "--out-dir",
        process.cwd() + "/temp/out",
      ];

      await this.runShellCommand("cargo", args);
      outputProcess = process.cwd() + "/temp/out/tari_console_wallet";
    }
    return outputProcess;
  }

  stop() {
    return new Promise((resolve) => {
      let name = this.name;
      if (!this.ps) {
        return resolve();
      }
      this.ps.on("close", (code) => {
        if (code) {
          console.log(`child process (${name}) exited with code ${code}`);
        }
        resolve();
      });
      this.ps.kill("SIGINT");
    });
  }

  async start(opts = {}) {
    const args = [
      "--base-path",
      ".",
      "--password",
      opts.password || "kensentme",
      "--seed-words-file-name",
      this.seedWordsFile,
      "--non-interactive",
      "--network",
      opts.network || (this.options || {}).network || "localnet",
    ];
    if (this.recoverWallet) {
      args.push("--recover", "--seed-words", this.seedWords);
    }
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    const overrides = Object.assign(this.getOverrides(), opts.config);
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    return await this.run(await this.compile(), args, true);
  }

  async changePassword(oldPassword, newPassword) {
    const args = [
      "--base-path",
      ".",
      "--password",
      oldPassword,
      "--update-password",
      "--network",
      "localnet",
    ];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    // Set input_buffer to double confirmation of the new password
    return await this.run(
      await this.compile(),
      args,
      true,
      newPassword + "\n" + newPassword + "\n"
    );
  }

  async runCommand(command) {
    // we need to quit the wallet before running a command
    await this.stop();
    const args = [
      "--base-path",
      ".",
      "--password",
      "kensentme",
      "--command",
      command,
      "--non-interactive",
      "localnet",
    ];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    let output = { buffer: "" };
    // In case we killed the wallet fast send enter. Because it will ask for the logs again (e.g. whois test)
    await this.run(await this.compile(), args, true, "\n", output, true);
    return output;
  }

  runShellCommand(cmd, args, opts = { env: {} }) {
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

  async exportSpentOutputs() {
    await this.stop();
    const args = [
      "--base-path",
      ".",
      "--auto-exit",
      "--password",
      "kensentme",
      "--network",
      "localnet",
      "--command",
      "export-spent-utxos --csv-file exported_outputs.csv",
    ];
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    let output = { buffer: "" };
    outputProcess = __dirname + "/../temp/out/tari_console_wallet";
    await this.run(outputProcess, args, true, "\n", output, true);
  }

  async exportUnspentOutputs() {
    await this.stop();
    const args = [
      "--base-path",
      ".",
      "--auto-exit",
      "--password",
      "kensentme",
      "--network",
      "localnet",
      "--command",
      "export-utxos --csv-file exported_outputs.csv",
    ];
    const overrides = this.getOverrides();
    Object.keys(overrides).forEach((k) => {
      args.push("-p");
      args.push(`${k}=${overrides[k]}`);
    });
    let output = { buffer: "" };
    outputProcess = __dirname + "/../temp/out/tari_console_wallet";
    await this.run(outputProcess, args, true, "\n", output, true);
  }

  async readExportedOutputs() {
    const filePath = path.resolve(this.baseDir + "/exported_outputs.csv");
    expect(fs.existsSync(filePath)).to.equal(
      true,
      "outputs export csv must exist"
    );

    let unblinded_outputs = await new Promise((resolve) => {
      let unblinded_outputs = [];
      fs.createReadStream(filePath)
        .pipe(csvParser())
        .on("data", (row) => {
          let unblinded_output = {
            value: parseInt(row.value),
            spending_key: Buffer.from(row.spending_key, "hex"),
            features: {
              flags: 0,
              maturity: parseInt(row.maturity) || 0,
              recovery_byte: parseInt(row.recovery_byte),
            },
            script: Buffer.from(row.script, "hex"),
            input_data: Buffer.from(row.input_data, "hex"),
            script_private_key: Buffer.from(row.script_private_key, "hex"),
            sender_offset_public_key: Buffer.from(
              row.sender_offset_public_key,
              "hex"
            ),
            metadata_signature: {
              public_nonce_commitment: Buffer.from(row.public_nonce, "hex"),
              signature_u: Buffer.from(row.signature_u, "hex"),
              signature_v: Buffer.from(row.signature_v, "hex"),
            },
          };
          unblinded_outputs.push(unblinded_output);
        })
        .on("end", () => {
          resolve(unblinded_outputs);
        });
    });

    return unblinded_outputs;
  }

  // Faucet outputs are only provided with an amount and spending key so we zero out the other output data
  // and update the input data to be the public key of the spending key, make the script private key the spending key
  // and then we can test if this output is still spendable when imported into the wallet.
  async readExportedOutputsAsFaucetOutputs() {
    let outputs = await this.readExportedOutputs();
    for (let i = 0; i < outputs.length; i++) {
      outputs[i].metadata_signature = {
        public_nonce_commitment: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
        signature_u: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
        signature_v: Buffer.from(
          "0000000000000000000000000000000000000000000000000000000000000000",
          "hex"
        ),
      };
      outputs[i].sender_offset_public_key = Buffer.from(
        "0000000000000000000000000000000000000000000000000000000000000000",
        "hex"
      );
      outputs[i].script_private_key = outputs[i].spending_key;
      let scriptPublicKey = tari_crypto.pubkey_from_secret(
        outputs[i].spending_key.toString("hex")
      );
      let input_data = Buffer.concat([
        Buffer.from([0x04]),
        Buffer.from(scriptPublicKey, "hex"),
      ]);
      outputs[i].input_data = input_data;
    }
    return outputs;
  }
}

module.exports = WalletProcess;
