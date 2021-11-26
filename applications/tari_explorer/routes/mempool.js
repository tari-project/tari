var { createClient } = require("../baseNodeClient");

var express = require("express");
var router = express.Router();

/* GET mempool page. */
router.get("/:tx", async function (req, res, next) {
  let tx = JSON.parse(Buffer.from(req.params.tx, "base64"));
  console.log("========== stringify 2 ========");
  console.log(tx.inputs);
  console.log("===============");
  res.render("Mempool", {
    inputs: tx.inputs,
    outputs: tx.outputs,
    kernels: tx.kernels,
  });
});

module.exports = router;
