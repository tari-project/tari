// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

var { Client } = require("base-node-grpc-client");

function createClient() {
  return Client.connect("localhost:18142");
}

module.exports = {
  createClient,
};
