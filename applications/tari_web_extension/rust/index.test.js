// requires a running wallet with grpc enabled
const { Client } = require("wallet-grpc-client");

const host = process.env.HOST || "127.0.0.1";
const port = process.env.PORT || "18143";
const address = `${host}:${port}`;
const wallet = Client.connect(address);

test("identify", async () => {
  const { public_key, public_address, node_id } = await wallet.identify();

  expect(public_key).toBeDefined();
  expect(public_key.length).toBe(64);

  expect(public_address).toBeDefined();
  expect(typeof public_address).toEqual("string");

  expect(node_id).toBeDefined();
  expect(node_id.length).toBe(26);
});
