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
    this.nodeAddress = baseNodeAddress.split(":")[0];
    this.nodeGrpcPort = baseNodeAddress.split(":")[1];
    this.baseNodeClient = baseNodeClient;
    this.walletAddress = walletAddress.split(":")[0];
    this.walletGrpcPort = walletAddress.split(":")[1];
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

  run(cmd, args, monerodUrl) {
    return new Promise((resolve, reject) => {
      if (!fs.existsSync(this.baseDir)) {
        fs.mkdirSync(this.baseDir, { recursive: true });
        fs.mkdirSync(this.baseDir + "/log", { recursive: true });
      }

      const proxyAddress = "127.0.0.1:" + this.port;

      const envs = createEnv(
        this.name,
        false,
        "nodeid.json",
        this.walletAddress,
        this.walletGrpcPort,
        this.port,
        this.nodeAddress,
        this.nodeGrpcPort,
        this.baseNodePort,
        proxyAddress,
        "127.0.0.1:8085",
        [],
        []
      );
      const extraEnvs = {
        TARI_MERGE_MINING_PROXY__LOCALNET__PROXY_SUBMIT_TO_ORIGIN:
          this.submitOrigin,
        TARI_MERGE_MINING_PROXY__LOCALNET__monerod_url: monerodUrl,
      };
      const completeEnvs = { ...envs, ...extraEnvs };
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

  async testWebsite(protocol, address, port, path) {
    const url = protocol + "://" + address + ":" + port;
    const webRequest = require(protocol);

    let request;
    let thePromise;
    const displayData = false;
    try {
      thePromise = await new Promise((resolve, reject) => {
        request = webRequest
          .get(url + path, (resp) => {
            let data = "";
            // Read all data chunks until the end
            resp.on("data", (chunk) => {
              data += chunk;
            });
            // Finish when complete response has been received
            resp.on("end", () => {
              if (displayData) {
                console.log(data); // `data` is 'used' here to keep eslint happy
              }
              return resolve(true);
            });
          })
          .on("error", () => {
            return reject(false);
          });
      });
      console.log(
        "  >> Info: `monerod` at",
        url,
        "is responsive and available"
      );
    } catch {
      console.log("  >> Warn: `monerod` at", url, "is not available!");
    }
    request.end();

    return thePromise;
  }

  async getMoneroStagenetUrl() {
    // See: https://monero.fail/?nettype=stagenet
    const monerodUrl = [
      ["http", "singapore.node.xmr.pm", "38081"],
      ["http", "stagenet.xmr-tw.org", "38081"],
      ["http", "xmr-lux.boldsuck.org", "38081"],
      ["http", "monero-stagenet.exan.tech", "38081"],
      ["http", "3.104.4.129", "18081"], // flaky
      ["http", "stagenet.community.xmr.to", "38081"], // flaky
      ["http", "super.fast.node.xmr.pm", "38089"], // flaky
    ];
    let url;
    for (let i = 0; i < monerodUrl.length; i++) {
      let availble = await this.testWebsite(
        monerodUrl[i][0],
        monerodUrl[i][1],
        monerodUrl[i][2],
        "/get_height"
      );
      if (availble) {
        url =
          monerodUrl[i][0] + "://" + monerodUrl[i][1] + ":" + monerodUrl[i][2];
        break;
      }
    }
    return url;
  }

  async startNew() {
    await this.init();
    const args = ["--base-path", ".", "--init"];
    if (this.logFilePath) {
      args.push("--log-config", this.logFilePath);
    }

    let url = await this.getMoneroStagenetUrl();
    return await this.run(await this.compile(), args, url);
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
