# Security assumptions

This file records the trust assumptions the code in this repository is built on. It exists so that people and
automated tools reviewing the code can tell a deliberate design choice from a defect.

It complements [SECURITY.md](../SECURITY.md), which describes how to report an issue and what is in scope.

## How to use this file

**If you are reviewing or scanning the code:**

- Read the entries below before you report a security finding.
- A finding that only works by breaking a listed assumption is expected behaviour, not a defect. Do not report it
  as one.
- A finding is still valid if it falls under an entry's **Not covered** line, or if you can show that the assumption
  itself is wrong or no longer matches the code. In that case cite the entry id and say why.
- If you rely on an assumption that is not listed here, say so in your report so it can be added or rejected.

**If you are changing the code:**

- A change that weakens or removes a listed assumption must update this file in the same pull request.
- A change that introduces a new trust relationship (one component accepting another's data or instructions without
  checking them) should add an entry.

## Adding or changing an entry

- Entries are added by pull request and count as decided once merged. Do not add an entry for behaviour that is still
  under discussion or has a fix in progress.
- Each entry has a stable id (`A-<AREA>-<NN>`). Ids are never reused; a withdrawn entry is moved to
  [Withdrawn entries](#withdrawn-entries) with the reason.
- Keep the fields below. **Not covered** is required: it states what the assumption does not excuse.

```
### A-<AREA>-<NN>: <one-line statement>

- **Assumption:** what is trusted, and by whom.
- **What that means:** what the trusted party can do as a result.
- **Not covered:** what the trusted party must still not be able to do.
- **Where:** the crates or modules it applies to.
- **Recorded:** date, and the pull request or issue where it was decided.
```

## Entries

### A-WALLET-01: The wallet trusts the base node it connects to for chain data

- **Assumption:** A wallet relies on its configured base node for the state of the chain: which outputs exist, which
  transactions are mined, the tip height and fee information. It does not validate the chain itself.
- **What that means:** A base node that lies can hide outputs or transactions from the wallet, report wrong
  confirmation status or balances, withhold submitted transactions, and learn which outputs the wallet asks about.
  Users who need stronger guarantees run their own base node.
- **Not covered:** The base node must not be able to obtain key material, make the wallet sign or broadcast a
  transaction the user did not ask for, crash the wallet, exhaust its memory or disk, or corrupt its database. Data
  from the base node is still untrusted input for parsing and size limits.
- **Where:** `base_layer/wallet`, `base_layer/transaction_components` (scanning and transaction submission clients),
  `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-SIGNER-01: Running the offline signer without a prompt is the operator's choice

- **Assumption:** The offline signer can run interactively, where a person confirms each request, or
  non-interactively, where it signs without asking. Choosing the non-interactive mode is a deployment decision, and
  the operator is then responsible for controlling what is delivered to the signer.
- **What that means:** In non-interactive mode there is no per-request human approval. The signer signs every
  request that passes its own checks. The console wallet's offline-payload signing commands and
  `sign-script-message` follow the same rule, and treat stdin that is not a terminal as non-interactive.
- **Not covered:** The checks the signer applies in either mode must hold, the summary shown in interactive mode must
  describe everything that is signed, and the interactive mode must not be bypassable without the operator selecting
  the non-interactive mode.
- **Where:** `applications/minotari_offline_signer`, `base_layer/transaction_components` (`offline_signing`),
  `applications/minotari_console_wallet` (offline-payload signing commands, `sign-script-message`).
- **Recorded:** 2026-11-17.

### A-SIGNER-02: The online wallet trusts the signed-result file it is given to submit

- **Assumption:** The channel that carries a signed-result file from the offline signer back to the online wallet is
  trusted. At submit the wallet does not verify the file's payload signature, which does not cover the signed
  transaction anyway, and does not bind the signed transaction to the prepared request.
- **What that means:** Whoever can replace the file can have the wallet broadcast any transaction the signer has
  signed, including one for a payload the operator cancelled, and record it with another prepared payload's
  recipient and amount. When nothing is locked under the file's tx_id, the file can lock the `Unspent` outputs its
  transaction spends until the broadcast is rejected. It can release the locks of the prepared payload it names by
  pairing its request with a transaction that is rejected. Closing this needs the payload signature verified at
  submit, the signed inputs, outputs and tx_id required to match the request, and the signed transaction validated
  before anything is locked.
- **Not covered:** A tampered file releasing outputs other than those locked under the prepared tx_id it names,
  creating records under tx_ids the wallet already has, or adding a wallet output whose commitment does not open to
  its stated value.
- **Where:** `base_layer/wallet` (`transaction_service`, `output_manager_service`),
  `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-24.

### A-MULTISIG-01: Multisig co-signers trust the party that builds the spending transaction

- **Assumption:** A co-signer's signature covers the message fixed in the output's script when the output was
  created. It does not cover the recipient, amount or fee of the transaction that later spends the output.
  Co-signers trust whoever assembles that transaction.
- **What that means:** Once the required number of signatures exists, the party building the transaction decides
  where the funds go. A signature is valid for every output whose script has the same message and key, so
  co-signers should take the message from an output they know independently, not only from the party asking.
  With `--output-hash`, the console wallet's `sign-script-message` refuses unless its base node has that output
  unspent with this message and key; that does not tie the signature to it.
- **Not covered:** Spending with fewer signatures than the threshold, using a signature for a different output's
  script message, or a non-member producing an accepted signature.
- **Where:** `base_layer/transaction_components` (`multisig`, `key_manager`), `base_layer/wallet`,
  `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-NET-01: One process serves one network

- **Assumption:** The network (mainnet, nextnet, a testnet, and so on) is set once when the process starts. Consensus
  hashing and domain separation read that process-wide value.
- **What that means:** Code that hashes or verifies consensus data without passing a network explicitly uses the
  network of the running process. Using data from another network in the same process is not supported.
- **Not covered:** Data from a remote peer on a different network being accepted as valid on this one.
- **Where:** `base_layer/transaction_components` (consensus hashers), `common`, `hashing`.
- **Recorded:** 2026-11-17.

### A-GRPC-01: A gRPC client with valid credentials is trusted with the methods it can reach

- **Assumption:** The base node and wallet gRPC interfaces are administrative interfaces. Whoever can connect with
  valid credentials, to the methods the operator enabled, is acting for the operator. The operator is responsible
  for the bind address, authentication, TLS and the list of enabled methods.
- **What that means:** An authenticated wallet gRPC client can spend funds, and can release the inputs locked by a
  prepared offline-signing payload with `CancelTransaction`. An authenticated base node gRPC client
  can call every enabled method, including expensive ones.
- **Not covered:** Reaching a method without the configured credentials, reaching a method the operator disabled,
  and anything an unauthenticated or remote peer-to-peer party can cause through these interfaces. Read-only methods
  the operator exposes publicly must stay bounded in cost.
- **Where:** `applications/minotari_app_grpc`, `applications/minotari_node`, `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-HTTP-01: The base node wallet query HTTP service is a public, unauthenticated service

- **Assumption:** The base node's wallet query HTTP service exists so that wallets can query a base node over the
  network. It is meant to be reachable by anyone, so it has no authentication and listens on all interfaces
  (`0.0.0.0`) unless `listen_ip` is set under `[base_node.http_wallet_query_service]`. Whether the port is reachable
  from outside the host is the operator's decision, made through `listen_ip`, firewall and router configuration.
- **What that means:** Anyone who can reach the port can read what the service returns (tip and header data, UTXO
  mined and spent status, transaction status, mempool fee statistics, burn output proofs) and can submit
  transactions through `/json_rpc`. All of this is public chain data or something any peer can already do over the
  peer-to-peer RPC interface, so unauthenticated access is not a privilege boundary. A node that is only reachable
  over Tor does not expose this port. The default `external_address` (`http://127.0.0.1:<port>`) is only the URL
  advertised to peers; it does not limit the bind address.
- **Not covered:** A caller must not be able to crash the node, exhaust its memory, CPU or disk, get a transaction
  into the mempool without full validation, or read anything that is not public chain or mempool data. Request
  bodies, query parameters and response sizes must stay bounded.
- **Where:** `applications/minotari_node` (`http`, `bootstrap.rs`, `config.rs`).
- **Recorded:** 2026-11-17, issue #7998.

### A-MCP-01: The MCP caller acts for the operator within the tools the server exposes

- **Assumption:** The client of the base node and wallet MCP servers (normally an AI agent) is acting for the
  operator, within the tools the server exposes. Read-only tools are always exposed. Control tools (submitting
  blocks and transactions, transfers, atomic swaps, coin splits, claims and refunds) are exposed only when the
  operator starts the server with control operations enabled (`control_enabled`). The operator is responsible for
  who can talk to the server's stdio and for deciding whether to enable control operations.
- **What that means:** With control operations enabled, the agent can submit blocks and transactions through the
  node, and can move wallet funds, without a per-call human confirmation (the wallet server refuses to start with
  `require_confirmation` and control operations both set). The agent may be prompt-injected, including by text
  that reaches it through tool results. Text that comes from upstream (gRPC error messages, peer user agents, ban
  reasons, addresses and protocols, wallet failure messages) is cleaned of control and invisible format characters,
  capped in length, and quoted or labelled as untrusted (`upstream: "..."`, `untrusted_peer_claims`); it is not
  neutralised and can still carry instructions. On the wallet side, the damage is limited by a cumulative spend
  budget per server process (default 1,000,000 Tari) and a `fee_per_gram` ceiling (default 10,000 µT/g) on every
  fund-moving call, both configurable. Transfers, atomic swaps and coin splits are charged their amount plus an
  upper-bound estimate of their fee; claims and refunds are charged their fee estimate. A call that fails after funds
  may have left the wallet is still charged, as is a call the wallet accepts but reports as failed, so failed or
  unconfirmed attempts can also use up the budget. The budget is held in memory and a server restart resets it. It
  only limits calls made through the MCP server: an agent or process that can reach the wallet's gRPC interface
  directly (the MCP server does not yet authenticate to it, so it must be left open to local clients; see A-GRPC-01),
  or that can restart the MCP server, can bypass or reset it.
- **Not covered:** Reaching a control tool when control operations are disabled, calling a gRPC method the server does
  not list as a tool, passing a discovered tool arguments its input schema does not accept, moving more than the
  configured spend budget or paying more than the fee ceiling through the MCP server, and making the server use
  unbounded memory through upstream data (stream results and peer text must stay bounded). A single request line from
  the client itself is not yet bounded by the stdio transport.
- **Where:** `applications/minotari_mcp_common`, `applications/minotari_mcp_node`, `applications/minotari_mcp_wallet`.
- **Recorded:** 2026-11-23.

### A-MMPROXY-01: The merge-mining proxy treats upstream monerod nodes as untrusted, and its listener as trusted miners only

- **Assumption:** The `monerod_url` entries, including the public defaults (many of them plain `http://`), are
  untrusted third parties. TLS to monerod deliberately skips certificate checks (PR #7060) so the proxy can fall back
  to raw IP addresses. The Tari coinbase and reward come from the local base node over gRPC, never from monerod. The
  listener (loopback by default) serves trusted miners; exposing it is the operator's choice.
- **What that means:** A monerod node can serve bad or stale templates or withhold responses, which costs
  merge-mining availability on that node; the proxy moves on to another configured monerod. Tari funds are not at
  risk, because the Tari coinbase and reward come from the base node. The Monero side is exposed: monerod builds the
  Monero block template, so an untrusted or on-path monerod can pay the Monero block reward to someone else, and it
  sees `monerod_username`/`monerod_password` when `monerod_use_auth` is set (in cleartext over `http://`, and over
  `https://` too because certificates are not checked). Operators who care should run their own monerod (see the
  SECURITY note in `common/config/presets/f_merge_mining_proxy.toml`). Requests the proxy does not recognise are
  forwarded to monerod, which should run with `--restricted-rpc`.
- **Not covered:** A monerod response must not be able to crash the proxy. Each response body is read under a fixed
  raw-byte limit (`MAX_MONEROD_RESPONSE_BYTES`, 8 MiB) and the per-request monerod timeout; the parsed JSON tree for
  a body within the limit can be many times larger per in-flight request, which is accepted. Upstream data must not
  change the Tari coinbase or the reward destination.
- **Where:** `applications/minotari_merge_mining_proxy` (`proxy/inner.rs`, `proxy/utils.rs`, `run_merge_miner.rs`,
  `config.rs`).
- **Recorded:** 2026-11-25, PR #TBD.

## Withdrawn entries

None.
