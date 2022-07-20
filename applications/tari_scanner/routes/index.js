// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var express = require("express");
const { client } = require("../baseNodeClient");
const { gen_table_route_path, gen_render_params } = require("../helpers/tables");
var router = express.Router();

router.get(
  gen_table_route_path("contracts") + gen_table_route_path("validator_nodes"),
  async function (req, res, next) {
    let tip = await client.getTip();
    res.render("index", {
      tip,
      ...gen_render_params(req, "contracts", "validator_nodes"),
      url: "",
    });
  }
);

module.exports = router;
