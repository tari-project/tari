// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// requires a running base node with grpc enabled

const { Client } = require("./index");
const host = process.env.HOST || "127.0.0.1";
const port = process.env.PORT || "18142";
const address = `${host}:${port}`;
const baseNode = Client.connect(address);

test("getVersion", async () => {
  const response = await baseNode.getVersion();
  const version = response.value;
  expect(version).toBeDefined();
  expect(version).toMatch(/\d\.\d.\d-\w+-\w+/); // eg: 0.9.6-9a0c999a6308be5c3ffbff78fe22d001b986815d-release
});

test("getTipInfo", async () => {
  const response = await baseNode.getTipInfo();
  expect(response.metadata).toBeDefined();
  const metadata = response.metadata;
  expect(metadata.height_of_longest_chain).toMatch(/\d+/);
});
