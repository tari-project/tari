// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import cloudflare from "cloudflare";
import debug from "debug";
import dotenv from "dotenv";
import { readFileSync } from "fs";
const log = debug("dns");
dotenv.config();

function getEnv() {
  if (!process.env.TOKEN) {
    throw new Error("No TOKEN set in env!");
  }
  if (!process.env.ZONE_ID) {
    throw new Error("No ZONE_ID set in env!");
  }
  if (!process.env.DOMAIN) {
    throw new Error("No DOMAIN set in env!");
  }

  return {
    token: process.env.TOKEN,
    zoneId: process.env.ZONE_ID,
    domain: process.env.DOMAIN,
    filePath: process.env.FILE || "../../meta/hashes.txt",
  };
}
const { token, zoneId, domain, filePath } = getEnv();

const client = cloudflare({
  token,
});

async function deleteRecords(records) {
  const del = records.map((record) => client.dnsRecords.del(zoneId, record.id));
  return await Promise.all(del);
}

async function addRecords(records) {
  let add = records.map((content) => {
    let record = {
      type: "TXT",
      name: domain,
      content: content,
      ttl: 1,
    };
    return client.dnsRecords.add(zoneId, record);
  });
  return await Promise.all(add);
}

const notEmptyOrComment = (line) => line && !line.trim().startsWith("#");

async function main(path) {
  try {
    log("Reading file ", path);
    const file = readFileSync(path, "utf8");
    const records = file.split("\n").filter(notEmptyOrComment);

    log("Updating DNS...");
    log("Fetch TXT records...");
    const { result } = await client.dnsRecords.browse(zoneId);
    const txt = result.filter((r) => r.type == "TXT");

    log(`Deleting ${txt.length} records...`);
    const deleted = await deleteRecords(txt);
    let ids = deleted.map((d) => d.result.id);
    log("Deleted record IDs: ", ids);

    log(`Adding ${records.length} records...`);
    log(records);

    const added = await addRecords(records);
    ids = added.map((a) => a.result.id);
    log("Added record IDs: ", ids);
    log("Done!");
  } catch (e) {
    console.error("Error: ", e);
  }
}

main(filePath);
