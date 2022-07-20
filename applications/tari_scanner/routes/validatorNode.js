// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var express = require("express");
var router = express.Router();
var { client } = require("../baseNodeClient");
const { gen_table_route_path, gen_render_params } = require("../helpers/tables");

/* GET home page. */
router.get("/:id" + gen_table_route_path("contracts"), async function (req, res, next) {
  let pub_key = req.params.id;
  let validator_node = await client.getValidatorNode(pub_key);
  res.render("validatorNode", {
    validator_node: validator_node,
    ...gen_render_params(req, "contracts"),
    url: `/validator_node/${pub_key}`,
  });
});

module.exports = router;
