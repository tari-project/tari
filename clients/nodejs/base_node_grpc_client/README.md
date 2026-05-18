# Tari Base Node GRPC client

Async GRPC client library for the Tari Base Node.

## Usage

```javascript
const { Client } = require("@tari/base-node-grpc-client");
const client = Client.connect("127.0.0.1:18142");
const { value } = await client.getVersion();
console.log(value);
```

## Development

- `npm install`
- hack hack hack

## Tests

- `npm test`
