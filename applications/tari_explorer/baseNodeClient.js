var {Client} = require("base-node-grpc-client");

function createClient() {
    return Client.connect("localhost:18142");
}

module.exports = {
    createClient
}

