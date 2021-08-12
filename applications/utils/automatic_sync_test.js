const BaseNodeProcess = require("../../integration_tests/helpers/baseNodeProcess");

const fs = require("fs/promises");

async function main() {
  const baseNode = new BaseNodeProcess("compile", true);
  await baseNode.init();

  // Set pruning horizon in config file if `pruned` command line arg is present
  if (process.argv.includes("pruned")) {
    let config = fs.readFileSync(baseNode.baseDir + "/config/config.toml");
    let updated_config = config
      .toString()
      .replace("#pruning_horizon = 0", "pruning_horizon = 1000");
    fs.writeFileSync(baseNode.baseDir + "/config/config.toml", updated_config);
  }

  await baseNode.start();

  await fs.mkdir("logs", { recursive: true });
  let logfile = await fs.open("logs/base-node.log", "w");
  let startTime = new Date();

  let syncPromise = new Promise((resolve) => {
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
  });

  let block_height = await syncPromise;

  let endTime = new Date();
  const timeDiffMs = endTime - startTime;
  const timeDiffMinutes = timeDiffMs / 60000;
  const block_rate = block_height / timeDiffMinutes;

  let message = "Syncing ";
  if (process.argv.includes("pruned")) {
    message = message + "Pruned Node ";
  } else {
    message = message + "Archival Node ";
  }

  console.log(
    message + "to block height",
    block_height,
    "took",
    timeDiffMinutes.toFixed(2),
    "minutes for a rate of",
    block_rate.toFixed(2),
    "blocks/min"
  );

  baseNode.stop();

  fs.rmdirSync(__dirname + "/temp/base_nodes", { recursive: true });
}

Promise.all([main()]);
