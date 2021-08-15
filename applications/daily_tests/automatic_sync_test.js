const BaseNodeProcess = require("integration_tests/helpers/baseNodeProcess");

const fs = require("fs/promises");
const path = require("path");
const { readLastNLines } = require("./helpers");

const SyncType = {
  Archival: "Archival",
  Pruned: "Pruned",
};

async function main() {
  let syncType;
  if (process.argv.includes("pruned")) {
    syncType = SyncType.Pruned;
  } else {
    syncType = SyncType.Archival;
  }

  try {
    const { blockHeight, timeDiffMinutes, blockRate } = await run({
      log: "./logs/base-node.log",
      syncType,
    });

    console.log(
      `${syncType} sync to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
    );
  } catch (err) {
    let logLines = readLastNLines("./logs/base-node.log", 10);
    console.log(
      `Syncing ${syncType} failed: ${err}
        Last 10 log lines:
        ${logLines}`
    );
  }
}

async function run(options) {
  const baseNode = new BaseNodeProcess("compile", true);
  await baseNode.init();

  // Set pruning horizon in config file if `pruned` command line arg is present
  if (options.syncType === SyncType.Pruned) {
    let config = await fs.readFile(baseNode.baseDir + "/config/config.toml");
    let updated_config = config
      .toString()
      .replace("#pruning_horizon = 0", "pruning_horizon = 1000");
    await fs.writeFile(
      baseNode.baseDir + "/config/config.toml",
      updated_config
    );
  }

  await baseNode.start();

  await fs.mkdir(path.dirname(options.log), { recursive: true });
  let logfile = await fs.open(options.log, "w+");
  let startTime = new Date();

  let syncPromise = new Promise((resolve, reject) => {
    baseNode.ps.stderr.on("data", (data) => {
      logfile.write(data);
    });
    baseNode.ps.stdout.on("data", (data) => {
      logfile.write(data);
      let height = parseInt(data.toString().match("Tip:\\ (\\d+)\\ \\(")[1]);

      if (
        parseInt(height) > 0 &&
        data
          .toString()
          .toUpperCase()
          .match(/STATE: LISTENING/)
      ) {
        resolve(height);
      }
    });
    baseNode.ps.once("error", (err) => {
      reject(err);
    });
  });

  let blockHeight = await syncPromise;

  let endTime = new Date();
  const timeDiffMs = endTime - startTime;
  const timeDiffMinutes = timeDiffMs / 60000;
  const blockRate = blockHeight / timeDiffMinutes;

  await baseNode.stop();

  try {
    await fs.rmdir(__dirname + "/temp/base_nodes", { recursive: true });
  } catch (err) {
    console.error(err);
  }
  return {
    blockRate: blockRate.toFixed(2),
    timeDiffMinutes: timeDiffMinutes.toFixed(2),
    blockHeight,
  };
}

if (require.main === module) {
  Promise.all([main()]);
} else {
  module.exports = run;
  Object.assign(module.exports, {
    SyncType,
  });
}
