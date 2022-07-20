// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var express = require("express");
var router = express.Router();
var { client } = require("../baseNodeClient");
const { gen_table_route_path, gen_render_params } = require("../helpers/tables");

/* GET home page. */
router.get("/:id" + gen_table_route_path("validator_nodes"), async function (req, res, next) {
  let pub_key = req.params.id;
  let contract = await client.getContract(pub_key);
  res.render("contract", {
    contract: contract,
    ...gen_render_params(req, "validator_nodes"),
    url: `/contract/${pub_key}`,
  });
});

module.exports = router;
