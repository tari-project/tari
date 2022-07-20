// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var express = require("express");
const { client } = require("../baseNodeClient");
const { contracts } = require("../helpers/contracts");
const { validator_nodes } = require("../helpers/validatorNodes");
var router = express.Router();

router.get("/contracts", async function (req, res, next) {
  console.log("/contracts", contracts);
  res.json(await client.getAllContracts());
});

router.get("/validator_nodes", async function (req, res, next) {
  res.json(await client.getAllValidatorNodes());
});

router.get("/validator/contracts/:id", async function (req, res, next) {
  console.log("/validator/contracts/:id");
  res.json((await client.getValidatorNode(req.params.id)).getAllContractsIds().map((id) => contracts.getContract(id)));
});

router.get("/contract/validator_nodes/:id", async function (req, res, next) {
  console.log("/contract/validator_nodes/:id");
  res.json(
    (await client.getContract(req.params.id))
      .getAllValidatorNodesIds()
      .map((id) => validator_nodes.getValidatorNode(id))
  );
});

module.exports = router;
