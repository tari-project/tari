// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var express = require("express");
const { createClient } = require("../baseNodeClient");
var router = express.Router();

/* GET mempool page. */
router.get("/:excessSigs", async function (req, res) {
  try {
    let client = createClient();
    let txId = req.params.excessSigs.split("+");
    console.log(txId);
    let mempool = await client.getMempoolTransactions({});
    let tx = null;
    for (let i = 0; i < mempool.length; i++) {
      for (let j = 0; j < mempool[i].transaction.body.kernels.length; j++) {
        for (let k = 0; k < txId.length; k++) {
          if (
            txId[k] ===
            Buffer.from(
              mempool[i].transaction.body.kernels[j].excess_sig.signature
            ).toString("hex")
          ) {
            tx = mempool[i].transaction;
            break;
          }
        }
        if (tx) {
          break;
        }
      }
    }

    if (!tx) {
      res.status(404);
      res.render("error", { error: "Tx not found" });
      return;
    }
    console.log(tx);
    console.log("===============");
    res.render("Mempool", {
      tx,
    });
  } catch (error) {
    res.status(500);
    res.render("error", { error: error });
  }
});

module.exports = router;
