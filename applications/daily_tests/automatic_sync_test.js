const fs = require("fs/promises");
const path = require("path");
const helpers = require("./helpers");
const BaseNodeProcess = require("integration_tests/helpers/baseNodeProcess");

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

  const { blockHeight, timeDiffMinutes, blockRate } = await run({
    log: "./logs/base-node.log",
    syncType,
  });

  console.log(
    `${syncType} sync to block height ${blockHeight} took ${timeDiffMinutes} minutes for a rate of ${blockRate} blocks/min`
  );
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

  try {
    let syncResult = await helpers.monitorProcessOutput({
      process: baseNode.ps,
      outputStream: logfile,
      onData: (data) => {
        // 13:50 v0.9.3, State: Listening, Tip: 20515 (Wed, 18 Aug 2021 08:17:25 +0000), Mempool: 2tx (60g, +/- 1blks), Connections: 17, Banned: 0, Messages (last 60s): 36, Rpc: 3/1000 sessions
        let match = data.match(/Tip: (\d+) \(/);
        if (!match) {
          return null;
        }
        let height = parseInt(match[1]);

        if (height > 0 && data.toUpperCase().match(/STATE: LISTENING/)) {
          return { height };
        }
      },
    });

    let endTime = new Date();
    const timeDiffMs = endTime - startTime;
    const timeDiffMinutes = timeDiffMs / 60000;
    const blockRate = syncResult.height / timeDiffMinutes;

    await baseNode.stop();

    return {
      blockRate: blockRate.toFixed(2),
      timeDiffMinutes: timeDiffMinutes.toFixed(2),
      blockHeight: syncResult.height,
    };
  } catch (err) {
    await baseNode.stop();
    throw err;
  }
}

if (require.main === module) {
  Promise.all([main()]);
} else {
  module.exports = run;
  Object.assign(module.exports, {
    SyncType,
  });
}
