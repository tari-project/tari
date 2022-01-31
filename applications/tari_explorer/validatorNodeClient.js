var { Client } = require("validator-node-grpc-client");

function createClient() {
  return Client.connect("localhost:18144");
}

module.exports = {
  createClient,
};
